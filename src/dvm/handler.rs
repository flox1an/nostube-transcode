use nostr_sdk::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::blossom::BlossomClient;
use crate::config::Config;
use crate::dvm::events::{
    build_result_event, build_status_event, build_status_event_with_eta, Codec, DvmResult, JobContext,
    JobStatus, Mp4Result, OutputMode,
};
use crate::error::DvmError;
use crate::nostr::EventPublisher;
use crate::video::{VideoMetadata, VideoProcessor};

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
        self.send_status(&job, JobStatus::Processing, Some("Starting video transformation"))
            .await?;

        // Validate input
        if job.input.input_type != "url" {
            return self
                .send_error(&job, "Only URL inputs are supported")
                .await;
        }

        // Process the video
        let result = self.process_video(&job).await;

        match result {
            Ok(dvm_result) => {
                info!(job_id = %job_id, result = ?dvm_result, "Job completed successfully");

                // Send result event
                let event = build_result_event(job_id, requester, &dvm_result);
                self.publisher.publish(event).await?;

                // Send success status
                self.send_status(&job, JobStatus::Success, Some("Video transformation complete"))
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
                let status_msg = format!("Transcoding to {} {} MP4", job.resolution.as_str(), codec_name);
                self.send_status(job, JobStatus::Processing, Some(&format!("{}...", status_msg))).await?;

                // Estimate: hardware encoding is roughly 2-5x realtime, use 3x as baseline
                let estimated_transcode_secs = (video_duration_secs / 3.0) as u64;

                // Transform with periodic progress updates
                // Use quality 15 for good quality on VideoToolbox (maps to q:v 70)
                let result = self
                    .run_with_progress(job, &status_msg, estimated_transcode_secs, self.processor.transform_mp4(input_url, job.resolution, Some(15), job.codec))
                    .await?;

                // Get file size for upload estimation
                let file_size = tokio::fs::metadata(&result.output_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                let upload_msg = "Uploading MP4 to all servers";
                self.send_status(job, JobStatus::Processing, Some(&format!("{}...", upload_msg))).await?;

                // Estimate: assume ~5 MB/s upload speed
                let estimated_upload_secs = file_size / (5 * 1024 * 1024);

                let blobs = self
                    .run_with_progress(job, upload_msg, estimated_upload_secs, self.blossom.upload_file_to_all(&result.output_path, "video/mp4"))
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
                // Get input height for resolution-aware transcoding
                let input_height = metadata.as_ref().ok().and_then(|m| m.resolution()).map(|(_, h)| h);
                let is_4k = input_height.map(|h| h >= 2160).unwrap_or(false);

                // Build status message based on expected output resolutions
                let resolution_list = if is_4k {
                    "360p, 720p, 1080p, 2160p"
                } else {
                    "360p, 720p, 1080p"
                };
                let codec_name = match job.codec {
                    Codec::H264 => "H.264",
                    Codec::H265 => "H.265",
                };
                let status_msg = format!("Transcoding to {} HLS ({})", codec_name, resolution_list);
                self.send_status(job, JobStatus::Processing, Some(&format!("{}...", status_msg))).await?;

                // Estimate: HLS with encoded streams + 1 copy
                // 4K has 3 encoded + 1 copy, non-4K has 2 encoded + 1 copy
                let encode_multiplier = if is_4k { 3.0 } else { 2.0 };
                let estimated_transcode_secs = (video_duration_secs / 3.0 * encode_multiplier) as u64;

                // Transform with periodic progress updates
                let (result, _transform_config) = self
                    .run_with_progress(job, &status_msg, estimated_transcode_secs, self.processor.transform(input_url, input_height, job.codec))
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
                self.send_status(job, JobStatus::Processing, Some(&format!("{}...", upload_msg))).await?;

                // Estimate: assume ~5 MB/s upload speed
                let estimated_upload_secs = total_size / (5 * 1024 * 1024);

                // Upload with periodic progress updates
                let hls_result = self
                    .run_with_progress(job, &upload_msg, estimated_upload_secs, self.blossom.upload_hls_output(&result))
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

        // Spawn a background task for periodic updates
        let progress_handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(10));
            ticker.tick().await; // First tick is immediate, skip it

            loop {
                ticker.tick().await;
                let elapsed = start.elapsed().as_secs();

                let (progress_msg, remaining_secs) = if estimated_secs > 0 {
                    let remaining = estimated_secs.saturating_sub(elapsed);
                    (format!("{} (~{} remaining)", message, format_duration(remaining)), Some(remaining))
                } else {
                    (format!("{} ({} elapsed)", message, format_duration(elapsed)), None)
                };

                let event = build_status_event_with_eta(job_id, requester, JobStatus::Processing, Some(&progress_msg), remaining_secs);
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

    async fn send_status(
        &self,
        job: &JobContext,
        status: JobStatus,
        message: Option<&str>,
    ) -> Result<(), DvmError> {
        let event = build_status_event(job.event_id(), job.requester(), status, message);
        self.publisher.publish(event).await?;
        Ok(())
    }

    async fn send_error(&self, job: &JobContext, message: &str) -> Result<(), DvmError> {
        let event = build_status_event(
            job.event_id(),
            job.requester(),
            JobStatus::Error,
            Some(message),
        );
        self.publisher.publish(event).await?;
        Err(DvmError::JobRejected(message.to_string()))
    }
}

/// Format duration in seconds to human-readable string
fn format_duration(secs: u64) -> String {
    if secs < 60 {
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
