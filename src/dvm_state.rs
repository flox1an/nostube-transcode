//! DVM state management with job tracking.
//!
//! Provides shared state for the DVM including configuration,
//! job statistics, and history.

use crate::remote_config::RemoteConfig;
use crate::dvm::events::JobContext;
use nostr_sdk::prelude::*;
use std::collections::{VecDeque, HashMap};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Maximum number of job records to keep in history
pub const MAX_JOB_HISTORY: usize = 100;

/// How long to keep a pending bid before timing out (5 minutes)
pub const PENDING_BID_TIMEOUT_SECS: u64 = 300;

/// Thread-safe shared DVM state
pub type SharedDvmState = Arc<RwLock<DvmState>>;

/// A bid sent by the DVM waiting for selection or payment
#[derive(Debug, Clone)]
pub struct PendingBid {
    pub context: JobContext,
    pub created_at: Instant,
}

/// DVM runtime state
#[derive(Debug)]
pub struct DvmState {
    /// Remote configuration
    pub config: RemoteConfig,
    /// DVM identity keys
    pub keys: Keys,
    /// When the DVM started
    pub started_at: Instant,
    /// Number of currently active jobs
    pub jobs_active: u32,
    /// Total completed jobs
    pub jobs_completed: u32,
    /// Total failed jobs
    pub jobs_failed: u32,
    /// Recent job history (newest first)
    pub job_history: VecDeque<JobRecord>,
    /// Bids sent to users waiting for selection/payment
    pub pending_bids: HashMap<EventId, PendingBid>,
    /// Hardware acceleration method if available
    pub hwaccel: Option<String>,
}

/// Record of a job execution
#[derive(Debug, Clone)]
pub struct JobRecord {
    /// Job ID
    pub id: String,
    /// Current status
    pub status: JobStatus,
    /// Input video URL
    pub input_url: String,
    /// Output URL (master playlist) if completed
    pub output_url: Option<String>,
    /// Unix timestamp when job started
    pub started_at: u64,
    /// Unix timestamp when job completed or failed
    pub completed_at: Option<u64>,
}

/// Job execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// Job is currently processing
    Processing,
    /// Job completed successfully
    Completed,
    /// Job failed
    Failed,
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStatus::Processing => write!(f, "processing"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
        }
    }
}

impl DvmState {
    /// Create a new DVM state
    pub fn new(keys: Keys, config: RemoteConfig) -> Self {
        Self {
            config,
            keys,
            started_at: Instant::now(),
            jobs_active: 0,
            jobs_completed: 0,
            jobs_failed: 0,
            job_history: VecDeque::new(),
            pending_bids: HashMap::new(),
            hwaccel: None,
        }
    }

    /// Add a pending bid
    pub fn add_bid(&mut self, context: JobContext) {
        let id = context.event_id();
        self.pending_bids.insert(id, PendingBid {
            context,
            created_at: Instant::now(),
        });
    }

    /// Remove and return a pending bid if it exists
    pub fn take_bid(&mut self, id: &EventId) -> Option<PendingBid> {
        self.pending_bids.remove(id)
    }

    /// Clean up expired bids
    pub fn cleanup_bids(&mut self) {
        let now = Instant::now();
        self.pending_bids.retain(|_, bid| {
            now.duration_since(bid.created_at).as_secs() < PENDING_BID_TIMEOUT_SECS
        });
    }

    /// Create a new shared DVM state
    pub fn new_shared(keys: Keys, config: RemoteConfig) -> SharedDvmState {
        Arc::new(RwLock::new(Self::new(keys, config)))
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Check if the DVM is paused
    pub fn is_paused(&self) -> bool {
        self.config.paused
    }

    /// Record a job starting
    pub fn job_started(&mut self, id: String, input_url: String) {
        self.jobs_active += 1;

        let record = JobRecord {
            id,
            status: JobStatus::Processing,
            input_url,
            output_url: None,
            started_at: Timestamp::now().as_u64(),
            completed_at: None,
        };

        // Add to front (newest first)
        self.job_history.push_front(record);

        // Trim history if needed
        while self.job_history.len() > MAX_JOB_HISTORY {
            self.job_history.pop_back();
        }
    }

    /// Record a job completing successfully
    pub fn job_completed(&mut self, id: &str, output_url: String) {
        self.jobs_active = self.jobs_active.saturating_sub(1);
        self.jobs_completed += 1;

        // Update the record in history
        if let Some(record) = self.job_history.iter_mut().find(|r| r.id == id) {
            record.status = JobStatus::Completed;
            record.output_url = Some(output_url);
            record.completed_at = Some(Timestamp::now().as_u64());
        }
    }

    /// Record a job failing
    pub fn job_failed(&mut self, id: &str) {
        self.jobs_active = self.jobs_active.saturating_sub(1);
        self.jobs_failed += 1;

        // Update the record in history
        if let Some(record) = self.job_history.iter_mut().find(|r| r.id == id) {
            record.status = JobStatus::Failed;
            record.completed_at = Some(Timestamp::now().as_u64());
        }
    }

    /// Get recent job history (newest first)
    pub fn get_job_history(&self, limit: usize) -> Vec<&JobRecord> {
        self.job_history.iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> Keys {
        Keys::generate()
    }

    #[test]
    fn test_new_state() {
        let keys = test_keys();
        let config = RemoteConfig::new();
        let state = DvmState::new(keys.clone(), config);

        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_completed, 0);
        assert_eq!(state.jobs_failed, 0);
        assert!(state.job_history.is_empty());
        assert_eq!(state.keys.public_key(), keys.public_key());
    }

    #[test]
    fn test_job_lifecycle() {
        let keys = test_keys();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        // Start a job
        state.job_started(
            "job1".to_string(),
            "https://example.com/video.mp4".to_string(),
        );
        assert_eq!(state.jobs_active, 1);
        assert_eq!(state.job_history.len(), 1);
        assert_eq!(state.job_history[0].status, JobStatus::Processing);

        // Complete the job
        state.job_completed(
            "job1",
            "https://blossom.example.com/master.m3u8".to_string(),
        );
        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_completed, 1);
        assert_eq!(state.job_history[0].status, JobStatus::Completed);
        assert!(state.job_history[0].output_url.is_some());
        assert!(state.job_history[0].completed_at.is_some());
    }

    #[test]
    fn test_job_failure() {
        let keys = test_keys();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        // Start a job
        state.job_started(
            "job1".to_string(),
            "https://example.com/video.mp4".to_string(),
        );
        assert_eq!(state.jobs_active, 1);

        // Fail the job
        state.job_failed("job1");
        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_failed, 1);
        assert_eq!(state.job_history[0].status, JobStatus::Failed);
        assert!(state.job_history[0].completed_at.is_some());
    }

    #[test]
    fn test_job_history_limit() {
        let keys = test_keys();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        // Add more jobs than the limit
        for i in 0..MAX_JOB_HISTORY + 10 {
            state.job_started(
                format!("job{}", i),
                format!("https://example.com/{}.mp4", i),
            );
        }

        // History should be capped at MAX_JOB_HISTORY
        assert_eq!(state.job_history.len(), MAX_JOB_HISTORY);

        // Newest jobs should be at the front
        assert_eq!(
            state.job_history[0].id,
            format!("job{}", MAX_JOB_HISTORY + 9)
        );

        // get_job_history should respect limit
        let history = state.get_job_history(5);
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_paused_state() {
        let keys = test_keys();
        let mut config = RemoteConfig::new();
        config.paused = false;

        let mut state = DvmState::new(keys, config);
        assert!(!state.is_paused());

        state.config.paused = true;
        assert!(state.is_paused());
    }
}
