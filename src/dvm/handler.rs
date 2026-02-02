use nostr_sdk::prelude::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::blossom::BlossomClient;
use crate::config::Config;
use crate::dvm::events::{
    build_result_event_encrypted, build_status_event_with_eta_encrypted, Codec, DvmResult,
    HlsResolution, JobContext, JobStatus, Mp4Result, OutputMode,
};
use crate::error::DvmError;
use crate::nostr::EventPublisher;
use crate::video::{TransformResult, VideoMetadata, VideoProcessor};

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
    #[allow(dead_code)]
    config: Arc<Config>,
    publisher: Arc<EventPublisher>,
    blossom: Arc<BlossomClient>,
    processor: Arc<VideoProcessor>,
}

impl JobHandler {
    pub fn new(
        config: Arc<Config>,
        publisher: Arc<EventPublisher>,
        blossom: Arc<BlossomClient>,
        processor: Arc<VideoProcessor>,
    ) -> Self {
        Self {
            config,
            publisher,
            blossom,
            processor,
        }
    }

    /// Process incoming jobs from the channel
    pub async fn run(&self, mut rx: mpsc::Receiver<JobContext>) {
        info!("Job handler started");

        while let Some(job) = rx.recv().await {
            let job_id = job.event_id();
            info!(job_id = %job_id, "Processing job");

            if let Err(e) = self.handle_job(job).await {
                error!(job_id = %job_id, error = %e, "Job failed");
            }
        }

        info!("Job handler stopped");
    }

    async fn handle_job(&self, job: JobContext) -> Result<(), DvmError> {
        let job_id = job.event_id();
        let requester = job.requester();

        // Send processing status
        self.send_status(
            &job,
            JobStatus::Processing,
            Some("Starting video transformation"),
        )
        .await?;

        // Validate input
        if job.input.input_type != "url" {
            return self.send_error(&job, "Only URL inputs are supported").await;
        }

        // Validate URL scheme (only allow HTTP/HTTPS for security)
        let input_url = &job.input.value;
        if !input_url.starts_with("http://") && !input_url.starts_with("https://") {
            return self
                .send_error(&job, "Only HTTP and HTTPS URLs are supported")
                .await;
        }

        // Process the video
        let result = self.process_video(&job).await;

        match result {
            Ok(dvm_result) => {
                info!(job_id = %job_id, result = ?dvm_result, "Job completed successfully");

                // Send result event (encrypted if request was encrypted)
                let event = build_result_event_encrypted(
                    job_id,
                    requester,
                    &dvm_result,
                    self.get_encryption_keys(&job),
                );
                self.publisher.publish(event).await?;

                // Send success status
                self.send_status(
                    &job,
                    JobStatus::Success,
                    Some("Video transformation complete"),
                )
                .await?;
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "Video processing failed");
                self.send_error(&job, &e.to_string()).await?;
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
                let codec_name = match job.codec {
                    Codec::H264 => "H.264",
                    Codec::H265 => "H.265",
                };
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

                // Estimate: hardware encoding is roughly 2-5x realtime, use 3x as baseline
                let estimated_transcode_secs = (video_duration_secs / 3.0) as u64;

                // Transform with periodic progress updates
                // Use quality 15 for good quality on VideoToolbox (maps to q:v 70)
                let result = self
                    .run_with_progress(
                        job,
                        &status_msg,
                        estimated_transcode_secs,
                        self.processor.transform_mp4(
                            input_url,
                            job.resolution,
                            Some(15),
                            job.codec,
                        ),
                    )
                    .await?;

                // Get file size for upload estimation
                let file_size = tokio::fs::metadata(&result.output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                // Total bytes = file_size * number_of_servers
                let num_servers = self.blossom.server_count();
                let total_upload_bytes = file_size * num_servers as u64;

                let upload_msg = format!(
                    "Uploading MP4 to {} server{}",
                    num_servers,
                    if num_servers == 1 { "" } else { "s" }
                );
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
                    HlsResolution::all()
                } else {
                    job.hls_resolutions.clone()
                };

                // Build status message based on selected resolutions
                let resolution_list: Vec<&str> =
                    selected_resolutions.iter().map(|r| r.as_str()).collect();
                let codec_name = match job.codec {
                    Codec::H264 => "H.264",
                    Codec::H265 => "H.265",
                };
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
                    .filter(|r| **r != HlsResolution::Original)
                    .count() as f64;
                let estimated_transcode_secs =
                    (video_duration_secs / 3.0 * encoded_count.max(1.0)) as u64;

                // Transform with periodic progress updates using user-selected resolutions
                let (result, _transform_config) = self
                    .run_with_progress(
                        job,
                        &status_msg,
                        estimated_transcode_secs,
                        self.processor.transform_with_resolutions(
                            input_url,
                            input_height,
                            job.codec,
                            &selected_resolutions,
                            source_codec.as_deref(),
                            job.encryption,
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

    /// Run a future with periodic progress updates every 10 seconds
    async fn run_with_progress<T, E, F>(
        &self,
        job: &JobContext,
        message: &str,
        estimated_secs: u64,
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
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        // Spawn a background task for periodic updates
        let progress_handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.tick().await; // First tick is immediate, skip it

            loop {
                ticker.tick().await;
                let elapsed = start.elapsed().as_secs();

                let (progress_msg, remaining_secs) = if estimated_secs > 0 {
                    let remaining = estimated_secs.saturating_sub(elapsed);
                    (
                        format!("{} (~{} remaining)", message, format_duration(remaining)),
                        Some(remaining),
                    )
                } else {
                    (
                        format!("{} ({} elapsed)", message, format_duration(elapsed)),
                        None,
                    )
                };

                let event = build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    remaining_secs,
                    encryption_keys.as_ref(),
                );
                if let Err(e) = publisher.publish(event).await {
                    debug!(error = %e, "Failed to send progress update");
                }
            }
        });

        // Run the actual operation
        let result = future.await;

        // Cancel the progress task
        progress_handle.abort();

        result
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
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        // Create shared atomic counter for real-time progress tracking
        let bytes_uploaded = Arc::new(AtomicU64::new(0));
        let bytes_for_task = bytes_uploaded.clone();
        let start_time = Instant::now();

        // Spawn a background task for periodic updates using real-time counter
        let progress_handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(3));
            ticker.tick().await; // First tick is immediate, skip it

            loop {
                ticker.tick().await;

                let uploaded = bytes_for_task.load(Ordering::Relaxed);
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

                let event = build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    if remaining_secs > 0 {
                        Some(remaining_secs)
                    } else {
                        None
                    },
                    encryption_keys.as_ref(),
                );
                if let Err(e) = publisher.publish(event).await {
                    debug!(error = %e, "Failed to send progress update");
                }
            }
        });

        // Run the upload with real-time progress tracking
        let result = self
            .blossom
            .upload_to_server_streaming_progress(path, mime_type, bytes_uploaded)
            .await
            .map_err(DvmError::Blossom);

        // Cancel the progress task
        progress_handle.abort();

        result
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
        let encryption_keys = if job.was_encrypted {
            Some(self.config.nostr_keys.clone())
        } else {
            None
        };

        // Create shared upload tracker
        let tracker = Arc::new(Mutex::new(UploadTracker::new(total_bytes)));
        let tracker_for_task = tracker.clone();

        // Spawn a background task for periodic updates using tracker
        let progress_handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.tick().await; // First tick is immediate, skip it

            loop {
                ticker.tick().await;

                let (remaining_secs, speed_mbps) = {
                    let t = tracker_for_task.lock().unwrap();
                    (
                        t.estimated_remaining_secs(),
                        t.average_speed() / (1024.0 * 1024.0),
                    )
                };

                let progress_msg = format!(
                    "{} (~{} remaining, {:.1} MB/s)",
                    message,
                    format_duration(remaining_secs),
                    speed_mbps
                );

                let event = build_status_event_with_eta_encrypted(
                    job_id,
                    requester,
                    JobStatus::Processing,
                    Some(&progress_msg),
                    Some(remaining_secs),
                    encryption_keys.as_ref(),
                );
                if let Err(e) = publisher.publish(event).await {
                    debug!(error = %e, "Failed to send progress update");
                }
            }
        });

        // Run the upload with progress callback
        let tracker_for_upload = tracker.clone();
        let result = self
            .blossom
            .upload_hls_output_with_progress(transform_result, |bytes, duration| {
                let mut t = tracker_for_upload.lock().unwrap();
                t.record_upload(bytes, duration.as_secs_f64());
            })
            .await
            .map_err(DvmError::Blossom);

        // Cancel the progress task
        progress_handle.abort();

        result
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
        let event = build_status_event_with_eta_encrypted(
            job.event_id(),
            job.requester(),
            status,
            message,
            None,
            keys,
        );
        self.publisher.publish(event).await?;
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
        );
        self.publisher.publish(event).await?;
        Err(DvmError::JobRejected(message.to_string()))
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
