use nostr_sdk::prelude::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::blossom::BlossomClient;
use crate::config::Config;
use crate::dvm_state::SharedDvmState;
use crate::dvm::events::{
    build_result_event_encrypted, build_status_event_with_eta_encrypted, build_status_event_with_context,
    Codec, DvmResult, JobContext, JobStatus, Mp4Result, OutputMode, CashuContext, Resolution,
};
use crate::error::DvmError;
use crate::nostr::EventPublisher;
use crate::video::{TransformResult, VideoMetadata, VideoProcessor};
use cdk::nuts::Token;
use cdk::amount::Amount;
use std::str::FromStr;

/// Default Cashu mint URL for payment requests
const CASHU_MINT_URL: &str = "https://mint.bitonic.nl";

/// DVM cost in satoshis (0 = free)
const DVM_COST_SATS: u64 = 0;

/// Tracks upload progress and dynamically estimates remaining time
#[derive(Debug)]
pub struct UploadTracker {
    bytes_uploaded: u64,
    total_bytes: u64,
    recent_speeds: VecDeque<f64>, // bytes per second
}

impl UploadTracker {
    const MAX_SAMPLES: usize = 10;
    const FALLBACK_SPEED: f64 = 5.0 * 1024.0 * 1024.0; // 5 MB/s

    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_uploaded: 0,
            total_bytes,
            recent_speeds: VecDeque::with_capacity(Self::MAX_SAMPLES),
        }
    }

    /// Record a completed upload and its duration
    pub fn record_upload(&mut self, bytes: u64, duration_secs: f64) {
        self.bytes_uploaded += bytes;
        if duration_secs > 0.0 {
            let speed = bytes as f64 / duration_secs;
            self.recent_speeds.push_back(speed);
            if self.recent_speeds.len() > Self::MAX_SAMPLES {
                self.recent_speeds.pop_front();
            }
        }
    }

    /// Get the average upload speed in bytes per second
    pub fn average_speed(&self) -> f64 {
        if self.recent_speeds.is_empty() {
            Self::FALLBACK_SPEED
        } else {
            self.recent_speeds.iter().sum::<f64>() / self.recent_speeds.len() as f64
        }
    }

    /// Estimate remaining seconds for upload
    pub fn estimated_remaining_secs(&self) -> u64 {
        let remaining_bytes = self.total_bytes.saturating_sub(self.bytes_uploaded);
        (remaining_bytes as f64 / self.average_speed()) as u64
    }

    /// Get current speed in MB/s
    #[allow(dead_code)]
    pub fn current_speed_mbps(&self) -> f64 {
        self.average_speed() / (1024.0 * 1024.0)
    }
}

pub struct JobHandler {
    config: Arc<Config>,
    state: SharedDvmState,
    publisher: Arc<EventPublisher>,
    blossom: Arc<BlossomClient>,
    processor: Arc<VideoProcessor>,
    http: reqwest::Client,
}

impl JobHandler {
    pub fn new(
        config: Arc<Config>,
        state: SharedDvmState,
        publisher: Arc<EventPublisher>,
        blossom: Arc<BlossomClient>,
        processor: Arc<VideoProcessor>,
    ) -> Self {
        Self {
            config,
            state,
            publisher,
            blossom,
            processor,
            http: reqwest::Client::new(),
        }
    }

    /// Process incoming jobs from the channel with configurable concurrency.
    ///
    /// Uses a semaphore to limit parallel job execution. The limit is read
    /// from `RemoteConfig::max_concurrent_jobs` (default: 1 for sequential).
    pub async fn run(self: Arc<Self>, mut rx: mpsc::Receiver<JobContext>) {
        // Read initial concurrency limit from config
        let max_jobs = {
            let state = self.state.read().await;
            state.config.max_concurrent_jobs.max(1)
        };
        let semaphore = Arc::new(Semaphore::new(max_jobs as usize));
        info!(max_concurrent_jobs = max_jobs, "Job handler started");

        while let Some(job) = rx.recv().await {
            // Acquire a semaphore permit before processing
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let handler = self.clone();
            tokio::spawn(async move {
                let job_id = job.event_id();
                let input_url = job.input.value.clone();
                info!(job_id = %job_id, "Processing job");

                // Track job start in state
                handler.state.write().await.job_started(
                    job_id.to_string(),
                    input_url,
                );

                match handler.handle_job(job).await {
                    Ok(()) => {
                        // Job completed successfully (result URL already sent in handle_job)
                    }
                    Err(e) => {
                        error!(job_id = %job_id, error = %e, "Job failed");
                        handler.state.write().await.job_failed(&job_id.to_string());
                    }
                }

                drop(permit);
            });
        }

        info!("Job handler stopped");
    }

    async fn handle_job(&self, job: JobContext) -> Result<(), DvmError> {
        let job_id = job.event_id();
        let requester = job.requester();
        let my_pubkey = self.config.nostr_keys.public_key();

        // Check if DVM is paused
        let is_paused = self.state.read().await.is_paused();
        if is_paused {
            return Ok(()); // Silently ignore requests when paused in Bid/Select mode
        }

        // Determine if this request is specifically for us
        let is_for_us = job.approved || job.request.tags.iter().any(|t| {
            let parts = t.as_slice();
            parts.len() >= 2 && parts[0] == "p" && parts[1] == my_pubkey.to_hex()
        });

        // Determine if it's addressed to someone else
        let is_for_others = job.request.tags.iter().any(|t| {
            let parts = t.as_slice();
            parts.len() >= 2 && parts[0] == "p" && parts[1] != my_pubkey.to_hex()
        });

        if !is_for_us {
            if is_for_others {
                // Addressed to someone else, ignore
                return Ok(());
            }

            return self.send_public_bid(job).await;
        }

        // If we got here, it's addressed to us (Selection).
        
        // Remove from pending bids if it was there (we are starting it now)
        self.state.write().await.take_bid(&job_id);
        
        // Define DVM cost
        let dvm_cost_sats = DVM_COST_SATS;
        let mint_url = CASHU_MINT_URL;

        if dvm_cost_sats > 0 {
            match job.cashu_token {
                Some(ref token_str) => {
                    info!(job_id = %job_id, "Verifying Cashu token...");
                    if let Err(e) = self.verify_cashu_token(token_str, dvm_cost_sats, mint_url).await {
                        warn!(job_id = %job_id, error = %e, "Cashu token verification failed");
                        return self.send_error(&job, &format!("Payment verification failed: {}", e)).await;
                    }
                    info!(job_id = %job_id, "Cashu token verified successfully");
                }
                None => {
                    warn!(job_id = %job_id, "Payment required but no Cashu token provided");
                    return self.send_cashu_bid(
                        &job,
                        mint_url,
                        dvm_cost_sats,
                        Some("Payment required to start this job"),
                    ).await;
                }
            }
        }

        info!(job_id = %job_id, "Starting execution for directed request");

        // Ensure job relays are in the client pool
        if !job.relays.is_empty() {
            self.publisher.ensure_relays_connected(&job.relays).await;
        }

        // Send immediate acknowledgment
        self.send_status(
            &job,
            JobStatus::Processing,
            Some("Job accepted, validating input..."),
        )
        .await?;

        self.validate_input(&job).await?;

        // Send processing status
        self.send_status(
            &job,
            JobStatus::Processing,
            Some("Starting video transformation"),
        )
        .await?;

        // Process the video
        let result = self.process_video(&job).await;

        match result {
            Ok(dvm_result) => {
                info!(job_id = %job_id, result = ?dvm_result, "Job completed successfully");

                // Extract output URL for state tracking
                let output_url = match &dvm_result {
                    DvmResult::Hls(hls) => hls.master_playlist.clone(),
                    DvmResult::Mp4(mp4) => mp4.urls.first().cloned().unwrap_or_default(),
                };

                // Send result event (encrypted if request was encrypted)
                let event = build_result_event_encrypted(
                    job_id,
                    requester,
                    &dvm_result,
                    self.get_encryption_keys(&job),
                );
                self.publisher.publish_for_job(event, &job.relays).await?;

                // Send success status
                self.send_status(
                    &job,
                    JobStatus::Success,
                    Some("Video transformation complete"),
                )
                .await?;

                // Track job completion in state
                self.state.write().await.job_completed(&job_id.to_string(), output_url);
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "Video processing failed");
                self.state.write().await.job_failed(&job_id.to_string());
                self.send_error(&job, &e.to_string()).await?;
            }
        }

        Ok(())
    }

    /// Send a bid for a public (non-directed) request
    async fn send_public_bid(&self, job: JobContext) -> Result<(), DvmError> {
        let job_id = job.event_id();
        debug!(job_id = %job_id, "Sending bid for public request");
        self.send_cashu_bid(
            &job,
            CASHU_MINT_URL,
            DVM_COST_SATS,
            Some("I can process this video for you"),
        )
        .await?;
        self.state.write().await.add_bid(job);
        Ok(())
    }

    /// Validate the input URL: type check, scheme check, and HEAD request
    async fn validate_input(&self, job: &JobContext) -> Result<(), DvmError> {
        if job.input.input_type != "url" {
            return self.send_error(job, "Only URL inputs are supported").await;
        }

        let input_url = &job.input.value;
        if !input_url.starts_with("http://") && !input_url.starts_with("https://") {
            return self
                .send_error(job, "Only HTTP and HTTPS URLs are supported")
                .await;
        }

        match self.http.head(input_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                debug!(url = %input_url, "URL is accessible");
            }
            Ok(resp) => {
                let err_msg = format!("Input URL returned status {}", resp.status());
                warn!(url = %input_url, error = %err_msg);
                return self.send_error(job, &err_msg).await;
            }
            Err(e) => {
                let err_msg = format!("Failed to reach input URL: {}", e);
                warn!(url = %input_url, error = %err_msg);
                return self.send_error(job, &err_msg).await;
            }
        }

        Ok(())
    }

    async fn process_video(&self, job: &JobContext) -> Result<DvmResult, DvmError> {
        let input_url = &job.input.value;

        debug!(url = %input_url, mode = ?job.mode, resolution = ?job.resolution, codec = ?job.codec, "Processing video");

        // Get video metadata for duration estimation
        let metadata = VideoMetadata::extract(input_url, &self.config.ffprobe_path).await;
        let video_duration_secs = metadata
            .as_ref()
            .ok()
            .and_then(|m| m.duration_secs())
            .unwrap_or(0.0);

        if let Err(e) = &metadata {
            warn!(error = %e, "Failed to get video metadata, progress estimates may be inaccurate");
        }

        match job.mode {
            OutputMode::Mp4 => {
                let codec_name = job.codec.friendly_name();
                let status_msg = format!(
                    "Transcoding to {} {} MP4",
                    job.resolution.as_str(),
                    codec_name
                );
                self.send_status(
                    job,
                    JobStatus::Processing,
                    Some(&format!("{}...", status_msg)),
                )
                .await?;

                // Estimate: conservatively assume 2x realtime for initial progress
                let estimated_transcode_secs = (video_duration_secs * 2.0) as u64;

                // Create shared atomic counter for real-time progress tracking from FFmpeg
                let progress_ms = Arc::new(AtomicU64::new(0));

                // Get source codec for decoder hint
                let source_codec = metadata
                    .as_ref()
                    .ok()
                    .and_then(|m| m.video_stream())
                    .and_then(|s| s.codec_name.clone());

                // Transform with periodic progress updates
                // Use quality 15 for good quality on VideoToolbox (maps to q:v 70)
                let result = self
                    .run_with_progress(
                        job,
                        &status_msg,
                        estimated_transcode_secs,
                        video_duration_secs,
                        progress_ms.clone(),
                        self.processor.transform_mp4(
                            input_url,
                            job.resolution,
                            Some(15),
                            job.codec,
                            source_codec.as_deref(),
                            Some(progress_ms),
                            Some(video_duration_secs),
                        ),
                    )
                    .await?;

                // Get file size for upload estimation
                let file_size = tokio::fs::metadata(&result.output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                // Total bytes = file_size * number_of_servers
                let num_servers = self.blossom.server_count().await;
                let total_upload_bytes = file_size * num_servers as u64;

                let upload_msg = format!(
                    "Uploading MP4 to {} server{}",
                    num_servers,
                    if num_servers == 1 { "" } else { "s" }
                );
                info!(path = %result.output_path.display(), size = file_size, "{}", upload_msg);
                self.send_status(
                    job,
                    JobStatus::Processing,
                    Some(&format!("{}...", upload_msg)),
                )
                .await?;

                let blobs = self
                    .run_single_file_upload_with_adaptive_progress(
                        job,
                        &upload_msg,
                        total_upload_bytes,
                        &result.output_path,
                        "video/mp4",
                    )
                    .await?;

                // Cleanup temp files
                result.cleanup().await;

                // Set mimetype based on codec
                let mimetype = match job.codec {
                    Codec::H264 => "video/mp4; codecs=\"avc1.64001f,mp4a.40.2\"",
                    Codec::H265 => "video/mp4; codecs=\"hvc1,mp4a.40.2\"",
                    Codec::AV1 => "video/mp4; codecs=\"av01.0.05M.08,opus\"", // Common AV1 MP4 mimetype (profile 0, level 5.0, Main)
                };

                Ok(DvmResult::Mp4(Mp4Result {
                    urls: blobs.into_iter().map(|b| b.url).collect(),
                    resolution: job.resolution.as_str().to_string(),
                    size_bytes: file_size,
                    mimetype: Some(mimetype.to_string()),
                }))
            }
            OutputMode::Hls => {
                // Get input height and codec for resolution-aware transcoding
                let input_height = metadata
                    .as_ref()
                    .ok()
                    .and_then(|m| m.resolution())
                    .map(|(_, h)| h);
                let source_codec = metadata
                    .as_ref()
                    .ok()
                    .and_then(|m| m.video_stream())
                    .and_then(|s| s.codec_name.clone());

                // Use user-selected resolutions (or all if not specified)
                let selected_resolutions = if job.hls_resolutions.is_empty() {
                    Resolution::all()
                } else {
                    job.hls_resolutions.clone()
                };

                // Build status message based on selected resolutions
                let resolution_list: Vec<&str> =
                    selected_resolutions.iter().map(|r| r.as_str()).collect();
                let codec_name = job.codec.friendly_name();
                let status_msg = format!(
                    "Transcoding to {} HLS ({})",
                    codec_name,
                    resolution_list.join(", ")
                );
                self.send_status(
                    job,
                    JobStatus::Processing,
                    Some(&format!("{}...", status_msg)),
                )
                .await?;

                // Estimate: count encoded streams (non-original resolutions)
                let encoded_count = selected_resolutions
                    .iter()
                    .filter(|r| **r != Resolution::Original)
                    .count() as f64;
                // Estimate: conservatively assume realtime encoding per resolution
                let estimated_transcode_secs =
                    (video_duration_secs * encoded_count.max(1.0)) as u64;

                // Create shared atomic counter for real-time progress tracking from FFmpeg
                let progress_ms = Arc::new(AtomicU64::new(0));

                // Transform with periodic progress updates using user-selected resolutions
                let (result, _transform_config) = self
                    .run_with_progress(
                        job,
                        &status_msg,
                        estimated_transcode_secs,
                        video_duration_secs,
                        progress_ms.clone(),
                        self.processor.transform_with_resolutions(
                            input_url,
                            input_height,
                            job.codec,
                            &selected_resolutions,
                            source_codec.as_deref(),
                            job.encryption,
                            Some(progress_ms),
                            Some(video_duration_secs),
                        ),
                    )
                    .await?;

                let total_files = result.segment_paths.len() + result.stream_playlists.len() + 1;

                // Estimate total size from segments
                let mut total_size: u64 = 0;
                for path in &result.segment_paths {
                    if let Ok(meta) = tokio::fs::metadata(path).await {
                        total_size += meta.len();
                    }
                }

                let upload_msg = format!("Uploading {} files to Blossom", total_files);
                info!(segment_count = result.segment_paths.len(), "{}", upload_msg);
                self.send_status(
                    job,
                    JobStatus::Processing,
                    Some(&format!("{}...", upload_msg)),
                )
                .await?;

                // Upload with adaptive progress tracking
                let hls_result = self
                    .run_upload_with_adaptive_progress(job, &upload_msg, total_size, &result)
                    .await?;

                // Cleanup temp files
                result.cleanup().await;

                Ok(DvmResult::Hls(hls_result))
            }
        }
    }

    /// Run a future with periodic progress updates every 5 seconds
    async fn run_with_progress<T, E, F>(
        &self,
        job: &JobContext,
        message: &str,
        estimated_secs: u64,
        total_duration_secs: f64,
        progress_ms: Arc<AtomicU64>,
        future: F,
    ) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let job_id = job.event_id();
        let requester = job.requester();
        let publisher = self.publisher.clone();
        let message = message.to_string();
        let job_relays = job.relays.clone();
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        run_with_ticker(
            publisher,
            job_relays,
            move || {
                let elapsed_secs = start.elapsed().as_secs();
                let actual_us = progress_ms.load(Ordering::Relaxed);
                // FFmpeg's out_time_ms is actually in microseconds despite the name
                let actual_secs = actual_us as f64 / 1_000_000.0;

                let (progress_msg, remaining_secs, progress_pct) = if actual_us > 0 && total_duration_secs > 0.0 {
                    let pct = ((actual_secs / total_duration_secs) * 100.0).min(99.0) as u32;
                    let speed = if elapsed_secs > 0 { actual_secs / elapsed_secs as f64 } else { 0.0 };
                    let remaining = if speed > 0.01 {
                        ((total_duration_secs - actual_secs) / speed) as u64
                    } else {
                        estimated_secs.saturating_sub(elapsed_secs)
                    };
                    (
                        format!("{} ({}%, ~{} remaining)", message, pct, format_duration(remaining)),
                        Some(remaining),
                        Some(pct),
                    )
                } else if estimated_secs > 0 {
                    let remaining = estimated_secs.saturating_sub(elapsed_secs);
                    let pct = ((elapsed_secs as f64 / estimated_secs as f64) * 100.0).min(99.0) as u32;
                    (
                        format!("{} (~{} remaining)", message, format_duration(remaining)),
                        Some(remaining),
                        Some(pct),
                    )
                } else {
                    (
                        format!("{} ({} elapsed)", message, format_duration(elapsed_secs)),
                        None,
                        None,
                    )
                };

                build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    remaining_secs,
                    encryption_keys.as_ref(),
                    progress_pct,
                )
            },
            future,
        )
        .await
    }

    /// Run single file upload with real-time progress tracking
    async fn run_single_file_upload_with_adaptive_progress(
        &self,
        job: &JobContext,
        message: &str,
        total_bytes: u64,
        path: &std::path::Path,
        mime_type: &str,
    ) -> Result<Vec<crate::blossom::BlobDescriptor>, DvmError> {
        let job_id = job.event_id();
        let requester = job.requester();
        let publisher = self.publisher.clone();
        let message = message.to_string();
        let job_relays = job.relays.clone();
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        let bytes_uploaded = Arc::new(AtomicU64::new(0));
        let bytes_for_tick = bytes_uploaded.clone();
        let start_time = Instant::now();

        run_with_ticker(
            publisher,
            job_relays,
            move || {
                let uploaded = bytes_for_tick.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed().as_secs_f64();

                let percent = if total_bytes > 0 {
                    ((uploaded as f64 / total_bytes as f64) * 100.0) as u32
                } else {
                    0
                };

                let speed_mbps = if elapsed > 0.0 {
                    (uploaded as f64 / elapsed) / (1024.0 * 1024.0)
                } else {
                    0.0
                };

                let remaining_secs = if speed_mbps > 0.0 {
                    let remaining_bytes = total_bytes.saturating_sub(uploaded);
                    (remaining_bytes as f64 / (speed_mbps * 1024.0 * 1024.0)) as u64
                } else {
                    0
                };

                let progress_msg = if remaining_secs > 0 && speed_mbps > 0.1 {
                    format!(
                        "{} ({}%, ~{} remaining @ {:.1} MB/s)",
                        message,
                        percent,
                        format_duration(remaining_secs),
                        speed_mbps
                    )
                } else if speed_mbps > 0.1 {
                    format!("{} ({}% @ {:.1} MB/s)", message, percent, speed_mbps)
                } else {
                    format!("{} ({}%)", message, percent)
                };

                build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    if remaining_secs > 0 { Some(remaining_secs) } else { None },
                    encryption_keys.as_ref(),
                    Some(percent),
                )
            },
            async {
                self.blossom
                    .upload_to_server_streaming_progress(path, mime_type, bytes_uploaded)
                    .await
                    .map_err(DvmError::Blossom)
            },
        )
        .await
    }

    /// Run HLS upload with adaptive progress tracking based on actual upload speeds
    async fn run_upload_with_adaptive_progress(
        &self,
        job: &JobContext,
        message: &str,
        total_bytes: u64,
        transform_result: &TransformResult,
    ) -> Result<crate::dvm::events::HlsResult, DvmError> {
        let job_id = job.event_id();
        let requester = job.requester();
        let publisher = self.publisher.clone();
        let message = message.to_string();
        let job_relays = job.relays.clone();
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        let tracker = Arc::new(Mutex::new(UploadTracker::new(total_bytes)));
        let tracker_for_tick = tracker.clone();
        let tracker_for_upload = tracker.clone();

        run_with_ticker(
            publisher,
            job_relays,
            move || {
                let (remaining_secs, speed_mbps, percent) = {
                    let t = tracker_for_tick.lock().unwrap();
                    let pct = if t.total_bytes > 0 {
                        ((t.bytes_uploaded as f64 / t.total_bytes as f64) * 100.0) as u32
                    } else {
                        0
                    };
                    (
                        t.estimated_remaining_secs(),
                        t.average_speed() / (1024.0 * 1024.0),
                        pct,
                    )
                };

                let progress_msg = format!(
                    "{} ({}%, ~{} remaining, {:.1} MB/s)",
                    message,
                    percent,
                    format_duration(remaining_secs),
                    speed_mbps
                );

                build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    Some(remaining_secs),
                    encryption_keys.as_ref(),
                    Some(percent),
                )
            },
            async {
                self.blossom
                    .upload_hls_output_with_progress(transform_result, move |bytes, duration| {
                        let mut t = tracker_for_upload.lock().unwrap();
                        t.record_upload(bytes, duration.as_secs_f64());
                    })
                    .await
                    .map_err(DvmError::Blossom)
            },
        )
        .await
    }


    async fn send_status(
        &self,
        job: &JobContext,
        status: JobStatus,
        message: Option<&str>,
    ) -> Result<(), DvmError> {
        // Use encryption if the request was encrypted
        let keys = if job.was_encrypted {
            Some(&self.config.nostr_keys)
        } else {
            None
        };

        debug!(
            job_id = %job.event_id(),
            status = ?status,
            message = ?message,
            "Sending status update"
        );

        let event = build_status_event_with_eta_encrypted(
            job.event_id(),
            job.requester(),
            status,
            message,
            None,
            keys,
            None,
        );
        self.publisher.publish_for_job(event, &job.relays).await?;
        Ok(())
    }

    async fn send_cashu_bid(
        &self,
        job: &JobContext,
        mint: &str,
        amount_sats: u64,
        message: Option<&str>,
    ) -> Result<(), DvmError> {
        let keys = if job.was_encrypted {
            Some(&self.config.nostr_keys)
        } else {
            None
        };

        let context = CashuContext {
            mint: mint.to_string(),
            amount_sats,
        };

        let event = build_status_event_with_context(
            job.event_id(),
            job.requester(),
            JobStatus::PaymentRequired,
            message,
            None,
            keys,
            Some(context),
            None,
        );

        self.publisher.publish_for_job(event, &job.relays).await?;
        Ok(())
    }

    async fn send_error(&self, job: &JobContext, message: &str) -> Result<(), DvmError> {
        // Use encryption if the request was encrypted
        let keys = if job.was_encrypted {
            Some(&self.config.nostr_keys)
        } else {
            None
        };
        let event = build_status_event_with_eta_encrypted(
            job.event_id(),
            job.requester(),
            JobStatus::Error,
            Some(message),
            None,
            keys,
            None,
        );
        self.publisher.publish_for_job(event, &job.relays).await?;
        Err(DvmError::JobRejected(message.to_string()))
    }

    /// Verifies a Cashu token with a mint.
    async fn verify_cashu_token(&self, token_str: &str, required_sats: u64, expected_mint: &str) -> Result<(), String> {
        let token = Token::from_str(token_str).map_err(|e| format!("Invalid Cashu token: {}", e))?;
        
        let mut total_amount = Amount::ZERO;

        match token {
            Token::TokenV3(v3) => {
                for token_proofs in &v3.token {
                    if token_proofs.mint.to_string() != expected_mint {
                        return Err(format!("Unexpected mint in V3: {} (expected {})", token_proofs.mint, expected_mint));
                    }
                    for proof in &token_proofs.proofs {
                        total_amount += proof.amount;
                    }
                }
            }
            Token::TokenV4(v4) => {
                if v4.mint_url.to_string() != expected_mint {
                    return Err(format!("Unexpected mint in V4: {} (expected {})", v4.mint_url, expected_mint));
                }
                for token_v4 in &v4.token {
                    for proof in &token_v4.proofs {
                        total_amount += proof.amount;
                    }
                }
            }
        }

        if total_amount < Amount::from(required_sats) {
            return Err(format!("Insufficient amount: {} (required {})", total_amount, required_sats));
        }

        // TODO: Contact the mint to verify the proofs are still valid (not spent)
        Ok(())
    }

    /// Get encryption keys if the job was encrypted
    fn get_encryption_keys(&self, job: &JobContext) -> Option<&Keys> {
        if job.was_encrypted {
            Some(&self.config.nostr_keys)
        } else {
            None
        }
    }
}

/// Runs an async operation while periodically publishing progress events every 5 seconds.
///
/// `make_event` is called every 5 seconds and returns a status event builder to publish.
async fn run_with_ticker<T, E, F, MakeEvent>(
    publisher: Arc<EventPublisher>,
    job_relays: Vec<url::Url>,
    make_event: MakeEvent,
    operation: F,
) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
    MakeEvent: Fn() -> EventBuilder + Send + 'static,
{
    let progress_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(20));
        ticker.tick().await; // First tick is immediate, skip it
        loop {
            ticker.tick().await;
            let event = make_event();
            if let Err(e) = publisher.publish_for_job(event, &job_relays).await {
                debug!(error = %e, "Failed to send progress update");
            }
        }
    });

    let result = operation.await;
    progress_handle.abort();
    result
}

/// Format duration in seconds to human-readable string
fn format_duration(secs: u64) -> String {
    if secs == 0 {
        "< 1s".to_string()
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        if remaining_secs > 0 {
            format!("{}m {}s", mins, remaining_secs)
        } else {
            format!("{}m", mins)
        }
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    }
}
