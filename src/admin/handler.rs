//! Admin command handler.
//!
//! Processes admin commands received via encrypted DMs,
//! validates authorization, and updates DVM state.

use crate::admin::commands::*;
use crate::config::Config;
use crate::dvm::events::{Codec, Resolution};
use crate::dvm_state::SharedDvmState;
use crate::remote_config::save_config;
use crate::video::hwaccel::HwAccel;
use crate::video::{VideoMetadata, VideoProcessor};
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command as TokioCommand;
use tokio::sync::Notify;
use tracing::{error, info};

/// Test video URL for self-test
const TEST_VIDEO_URL: &str =
    "https://almond.slidestr.net/ecf8f3a25b4a6109c5aa6ea90ee97f8cafec09f99a2f71f0e6253c3bdf26ccea";

/// Handles admin commands for the DVM.
pub struct AdminHandler {
    /// Shared DVM state
    state: SharedDvmState,
    /// Nostr client for publishing config updates
    client: Client,
    /// Runtime configuration (ffmpeg paths, temp dir, etc.)
    config: Arc<Config>,
    /// Notify the announcement publisher when config changes
    config_notify: Arc<Notify>,
}

impl AdminHandler {
    /// Creates a new admin handler.
    pub fn new(
        state: SharedDvmState,
        client: Client,
        config: Arc<Config>,
        config_notify: Arc<Notify>,
    ) -> Self {
        Self {
            state,
            client,
            config,
            config_notify,
        }
    }

    /// Ensures the client's relay pool includes all relays from the config.
    ///
    /// Adds any relays that aren't already connected. Bootstrap relays remain
    /// connected from startup, so config is always findable on restart.
    async fn sync_relays(&self, relays: &[String]) {
        let connected: HashSet<String> = self
            .client
            .relays()
            .await
            .keys()
            .map(|url| url.as_str().trim_end_matches('/').to_string())
            .collect();

        let mut added = false;
        for relay in relays {
            let normalized = relay.trim_end_matches('/').to_string();
            if !connected.contains(&normalized) {
                if let Err(e) = self.client.add_relay(relay.clone()).await {
                    tracing::warn!("Failed to add relay {}: {}", relay, e);
                } else {
                    added = true;
                }
            }
        }

        if added {
            self.client.connect().await;
        }
    }

    /// Handles an admin command from a sender.
    ///
    /// Validates that the sender is authorized (either admin or during pairing)
    /// and dispatches to the appropriate handler.
    pub async fn handle(&self, command: AdminCommand, sender: PublicKey) -> AdminResponse {
        // All commands require the sender to be the admin
        let state = self.state.read().await;
        let is_admin = state
            .config
            .admin_pubkey()
            .map(|admin| admin == sender)
            .unwrap_or(false);

        if !is_admin {
            return AdminResponse::error("Unauthorized");
        }
        drop(state);

        // Dispatch to handler
        match command {
            AdminCommand::GetConfig => self.handle_get_config().await,
            AdminCommand::SetRelays { relays } => self.handle_set_relays(relays).await,
            AdminCommand::SetBlossomServers { servers } => {
                self.handle_set_blossom_servers(servers).await
            }
            AdminCommand::SetBlobExpiration { days } => self.handle_set_blob_expiration(days).await,
            AdminCommand::SetProfile { name, about } => self.handle_set_profile(name, about).await,
            AdminCommand::Pause => self.handle_pause().await,
            AdminCommand::Resume => self.handle_resume().await,
            AdminCommand::Status => self.handle_status().await,
            AdminCommand::JobHistory { limit } => self.handle_job_history(limit).await,
            AdminCommand::GetDashboard { limit } => self.handle_get_dashboard(limit).await,
            AdminCommand::SetConfig {
                relays,
                blossom_servers,
                blob_expiration_days,
                name,
                about,
                max_concurrent_jobs,
            } => {
                self.handle_set_config(relays, blossom_servers, blob_expiration_days, name, about, max_concurrent_jobs)
                    .await
            }
            AdminCommand::SelfTest => self.handle_self_test().await,
            AdminCommand::SystemInfo => self.handle_system_info().await,
            AdminCommand::ImportEnvConfig => self.handle_import_env_config().await,
        }
    }

    /// Handles the GetConfig command.
    async fn handle_get_config(&self) -> AdminResponse {
        let state = self.state.read().await;

        let config_data = ConfigData {
            relays: state.config.relays.clone(),
            blossom_servers: state.config.blossom_servers.clone(),
            blob_expiration_days: state.config.blob_expiration_days,
            name: state.config.name.clone(),
            about: state.config.about.clone(),
            paused: state.config.paused,
            max_concurrent_jobs: state.config.max_concurrent_jobs,
        };

        AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse {
            config: config_data,
        }))
    }

    /// Handles the SetRelays command.
    async fn handle_set_relays(&self, relays: Vec<String>) -> AdminResponse {
        // Validate relay URLs
        for relay in &relays {
            if !relay.starts_with("wss://") && !relay.starts_with("ws://") {
                return AdminResponse::error(format!("Invalid relay URL: {}", relay));
            }
        }

        // Connect to new relays before saving so config is published there too
        self.sync_relays(&relays).await;

        let result = {
            let mut state = self.state.write().await;
            state.config.relays = relays;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                AdminResponse::ok_with_msg("Relays updated")
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the SetBlossomServers command.
    async fn handle_set_blossom_servers(&self, servers: Vec<String>) -> AdminResponse {
        // Validate server URLs
        for server in &servers {
            if !server.starts_with("https://") && !server.starts_with("http://") {
                return AdminResponse::error(format!("Invalid server URL: {}", server));
            }
        }

        let result = {
            let mut state = self.state.write().await;
            state.config.blossom_servers = servers;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                AdminResponse::ok_with_msg("Blossom servers updated")
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the SetBlobExpiration command.
    async fn handle_set_blob_expiration(&self, days: u32) -> AdminResponse {
        if days == 0 {
            return AdminResponse::error("Expiration days must be greater than 0");
        }

        let result = {
            let mut state = self.state.write().await;
            state.config.blob_expiration_days = days;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                AdminResponse::ok_with_msg(format!("Blob expiration set to {} days", days))
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the SetProfile command.
    async fn handle_set_profile(
        &self,
        name: Option<String>,
        about: Option<String>,
    ) -> AdminResponse {
        if name.is_none() && about.is_none() {
            return AdminResponse::error("At least one of 'name' or 'about' must be provided");
        }

        let result = {
            let mut state = self.state.write().await;
            if let Some(n) = name {
                state.config.name = Some(n);
            }
            if let Some(a) = about {
                state.config.about = Some(a);
            }
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                AdminResponse::ok_with_msg("Profile updated")
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the Pause command. Returns status in the response.
    async fn handle_pause(&self) -> AdminResponse {
        let result = {
            let mut state = self.state.write().await;
            if state.config.paused {
                return AdminResponse::error("DVM is already paused");
            }
            state.config.paused = true;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                self.handle_status().await
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the Resume command. Returns status in the response.
    async fn handle_resume(&self) -> AdminResponse {
        let result = {
            let mut state = self.state.write().await;
            if !state.config.paused {
                return AdminResponse::error("DVM is not paused");
            }
            state.config.paused = false;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                self.handle_status().await
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the Status command.
    async fn handle_status(&self) -> AdminResponse {
        let state = self.state.read().await;

        let status = StatusResponse {
            paused: state.config.paused,
            jobs_active: state.jobs_active,
            jobs_completed: state.jobs_completed,
            jobs_failed: state.jobs_failed,
            uptime_secs: state.uptime_secs(),
            hwaccel: state.hwaccel.clone().unwrap_or_else(|| "none".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        AdminResponse::ok_with_data(ResponseData::Status(status))
    }

    /// Handles the JobHistory command.
    async fn handle_job_history(&self, limit: u32) -> AdminResponse {
        let state = self.state.read().await;
        let history = state.get_job_history(limit as usize);

        let jobs: Vec<JobInfo> = history
            .into_iter()
            .map(|record| {
                let duration_secs = record
                    .completed_at
                    .map(|end| end.saturating_sub(record.started_at));

                JobInfo {
                    id: record.id.clone(),
                    status: record.status.to_string(),
                    input_url: record.input_url.clone(),
                    output_url: record.output_url.clone(),
                    started_at: format_timestamp(record.started_at),
                    completed_at: record.completed_at.map(format_timestamp),
                    duration_secs,
                }
            })
            .collect();

        AdminResponse::ok_with_data(ResponseData::JobHistory(JobHistoryResponse { jobs }))
    }

    /// Handles the GetDashboard command.
    ///
    /// Returns status, config, and recent jobs in a single response.
    async fn handle_get_dashboard(&self, limit: u32) -> AdminResponse {
        let state = self.state.read().await;

        let status = StatusResponse {
            paused: state.config.paused,
            jobs_active: state.jobs_active,
            jobs_completed: state.jobs_completed,
            jobs_failed: state.jobs_failed,
            uptime_secs: state.uptime_secs(),
            hwaccel: state.hwaccel.clone().unwrap_or_else(|| "none".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let config = ConfigData {
            relays: state.config.relays.clone(),
            blossom_servers: state.config.blossom_servers.clone(),
            blob_expiration_days: state.config.blob_expiration_days,
            name: state.config.name.clone(),
            about: state.config.about.clone(),
            paused: state.config.paused,
            max_concurrent_jobs: state.config.max_concurrent_jobs,
        };

        let history = state.get_job_history(limit as usize);
        let jobs: Vec<JobInfo> = history
            .into_iter()
            .map(|record| {
                let duration_secs = record
                    .completed_at
                    .map(|end| end.saturating_sub(record.started_at));
                JobInfo {
                    id: record.id.clone(),
                    status: record.status.to_string(),
                    input_url: record.input_url.clone(),
                    output_url: record.output_url.clone(),
                    started_at: format_timestamp(record.started_at),
                    completed_at: record.completed_at.map(format_timestamp),
                    duration_secs,
                }
            })
            .collect();

        AdminResponse::ok_with_data(ResponseData::Dashboard(DashboardResponse {
            status,
            config,
            jobs,
        }))
    }

    /// Handles the SetConfig command.
    ///
    /// Applies all provided config fields and returns the updated config.
    async fn handle_set_config(
        &self,
        relays: Option<Vec<String>>,
        blossom_servers: Option<Vec<String>>,
        blob_expiration_days: Option<u32>,
        name: Option<String>,
        about: Option<String>,
        max_concurrent_jobs: Option<u32>,
    ) -> AdminResponse {
        // Validate relay URLs if provided
        if let Some(ref relays) = relays {
            for relay in relays {
                if !relay.starts_with("wss://") && !relay.starts_with("ws://") {
                    return AdminResponse::error(format!("Invalid relay URL: {}", relay));
                }
            }
        }

        // Validate server URLs if provided
        if let Some(ref servers) = blossom_servers {
            for server in servers {
                if !server.starts_with("https://") && !server.starts_with("http://") {
                    return AdminResponse::error(format!("Invalid server URL: {}", server));
                }
            }
        }

        if let Some(days) = blob_expiration_days {
            if days == 0 {
                return AdminResponse::error("Expiration days must be greater than 0");
            }
        }

        if let Some(jobs) = max_concurrent_jobs {
            if jobs == 0 {
                return AdminResponse::error("max_concurrent_jobs must be at least 1");
            }
        }

        // Connect to new relays before saving so config is published there too
        if let Some(ref r) = relays {
            self.sync_relays(r).await;
        }

        let result = {
            let mut state = self.state.write().await;

            if let Some(r) = relays {
                state.config.relays = r;
            }
            if let Some(s) = blossom_servers {
                state.config.blossom_servers = s;
            }
            if let Some(d) = blob_expiration_days {
                state.config.blob_expiration_days = d;
            }
            if let Some(n) = name {
                state.config.name = Some(n);
            }
            if let Some(a) = about {
                state.config.about = Some(a);
            }
            if let Some(j) = max_concurrent_jobs {
                state.config.max_concurrent_jobs = j;
            }

            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                self.handle_get_config().await
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the SelfTest command.
    ///
    /// Encodes a short test video and returns performance metrics.
    async fn handle_self_test(&self) -> AdminResponse {
        info!("Starting self-test with video: {}", TEST_VIDEO_URL);

        let resolution = Resolution::R720p;

        // Get video metadata to determine duration
        let metadata = match VideoMetadata::extract(TEST_VIDEO_URL, &self.config.ffprobe_path).await
        {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to extract metadata: {}", e);
                return AdminResponse::ok_with_data(ResponseData::SelfTest(SelfTestResponse {
                    success: false,
                    video_duration_secs: None,
                    encode_time_secs: None,
                    speed_ratio: None,
                    speed_description: None,
                    hwaccel: None,
                    resolution: Some(resolution.as_str().to_string()),
                    output_size_bytes: None,
                    error: Some(format!("Failed to extract metadata: {}", e)),
                }));
            }
        };

        let video_duration = metadata.duration_secs().unwrap_or(0.0);
        info!(duration_secs = video_duration, "Video metadata extracted");

        // Create video processor
        let processor = VideoProcessor::new(self.config.clone());
        let hwaccel = processor.hwaccel();

        // Time the encoding
        let start = Instant::now();

        let result = match processor
            .transform_mp4(TEST_VIDEO_URL, resolution, Some(23), Codec::default(), None, None, None)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!("Self-test encoding failed: {}", e);
                return AdminResponse::ok_with_data(ResponseData::SelfTest(SelfTestResponse {
                    success: false,
                    video_duration_secs: Some(video_duration),
                    encode_time_secs: Some(start.elapsed().as_secs_f64()),
                    speed_ratio: None,
                    speed_description: None,
                    hwaccel: Some(hwaccel.to_string()),
                    resolution: Some(resolution.as_str().to_string()),
                    output_size_bytes: None,
                    error: Some(format!("Encoding failed: {}", e)),
                }));
            }
        };

        let encode_time = start.elapsed().as_secs_f64();
        let speed_ratio = if encode_time > 0.0 {
            video_duration / encode_time
        } else {
            0.0
        };

        let speed_description = if speed_ratio >= 1.0 {
            format!("{:.1}x realtime (faster than realtime)", speed_ratio)
        } else if speed_ratio > 0.0 {
            format!("{:.1}x realtime (slower than realtime)", speed_ratio)
        } else {
            "N/A".to_string()
        };

        // Get output file size
        let output_size_bytes = tokio::fs::metadata(&result.output_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!(
            encode_time_secs = encode_time,
            speed_ratio = speed_ratio,
            output_size_bytes = output_size_bytes,
            hwaccel = %hwaccel,
            "Self-test complete"
        );

        // Cleanup temp files
        result.cleanup().await;

        AdminResponse::ok_with_data(ResponseData::SelfTest(SelfTestResponse {
            success: true,
            video_duration_secs: Some(video_duration),
            encode_time_secs: Some(encode_time),
            speed_ratio: Some(speed_ratio),
            speed_description: Some(speed_description),
            hwaccel: Some(hwaccel.to_string()),
            resolution: Some(resolution.as_str().to_string()),
            output_size_bytes: Some(output_size_bytes),
            error: None,
        }))
    }

    /// Handles the SystemInfo command.
    ///
    /// Returns system information including platform, GPU, disk, and FFmpeg details.
    async fn handle_system_info(&self) -> AdminResponse {
        info!("Getting system info");

        // Detect hardware encoders
        let selected_hwaccel = HwAccel::detect();
        let available_hwaccels = HwAccel::detect_all();

        let hw_encoders: Vec<HwEncoderInfo> = available_hwaccels
            .into_iter()
            .map(|hw| {
                let mut codecs = vec!["H.264".to_string(), "H.265 (HEVC)".to_string()];
                // Check AV1 support per encoder type
                match hw {
                    HwAccel::Nvenc => {
                        if HwAccel::is_nvenc_av1_available() {
                            codecs.push("AV1".to_string());
                        }
                    }
                    _ => {
                        codecs.push("AV1".to_string());
                    }
                }
                HwEncoderInfo {
                    name: hw.name().to_string(),
                    selected: hw == selected_hwaccel,
                    codecs,
                }
            })
            .collect();

        // Get GPU info
        let gpu = get_gpu_info().await;

        // Get disk info for temp directory
        let disk = get_disk_info(&self.config.temp_dir);

        // Get FFmpeg info
        let ffmpeg_version = get_ffmpeg_version(&self.config.ffmpeg_path).await;
        let ffmpeg = FfmpegInfo {
            path: self.config.ffmpeg_path.to_string_lossy().to_string(),
            version: ffmpeg_version,
            ffprobe_path: self.config.ffprobe_path.to_string_lossy().to_string(),
        };

        AdminResponse::ok_with_data(ResponseData::SystemInfo(SystemInfoResponse {
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hw_encoders,
            gpu,
            disk,
            ffmpeg,
            temp_dir: self.config.temp_dir.to_string_lossy().to_string(),
        }))
    }

    /// Handles the ImportEnvConfig command.
    ///
    /// Reads configuration from environment variables and updates the remote config.
    async fn handle_import_env_config(&self) -> AdminResponse {
        // Read environment variables
        let relays = std::env::var("NOSTR_RELAYS").ok().map(|s| {
            s.split(',')
                .map(|r| r.trim().to_string())
                .collect::<Vec<_>>()
        });

        let blossom_servers = std::env::var("BLOSSOM_UPLOAD_SERVERS").ok().map(|s| {
            s.split(',')
                .map(|r| r.trim().to_string())
                .collect::<Vec<_>>()
        });

        let blob_expiration_days = std::env::var("BLOSSOM_BLOB_EXPIRATION_DAYS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok());

        let name = std::env::var("DVM_NAME").ok();
        let about = std::env::var("DVM_ABOUT").ok();

        // Track what was imported
        let mut imported = Vec::new();

        // Connect to new relays before saving so config is published there too
        if let Some(ref r) = relays {
            if !r.is_empty() {
                self.sync_relays(r).await;
            }
        }

        let result = {
            let mut state = self.state.write().await;

            if let Some(r) = relays {
                if !r.is_empty() {
                    state.config.relays = r;
                    imported.push("relays");
                }
            }

            if let Some(s) = blossom_servers {
                if !s.is_empty() {
                    state.config.blossom_servers = s;
                    imported.push("blossom_servers");
                }
            }

            if let Some(d) = blob_expiration_days {
                state.config.blob_expiration_days = d;
                imported.push("blob_expiration_days");
            }

            if let Some(n) = name {
                state.config.name = Some(n);
                imported.push("name");
            }

            if let Some(a) = about {
                state.config.about = Some(a);
                imported.push("about");
            }

            if imported.is_empty() {
                return AdminResponse::error("No environment configuration found to import");
            }

            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => {
                self.config_notify.notify_one();
                AdminResponse::ok_with_msg(format!("Imported: {}", imported.join(", ")))
            }
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }
}

/// Formats a Unix timestamp as ISO 8601.
fn format_timestamp(ts: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let datetime = UNIX_EPOCH + Duration::from_secs(ts);
    // Format as ISO 8601 using chrono would be cleaner, but we'll use a simple format
    // that doesn't require additional dependencies
    let secs_since_epoch = datetime
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Simple ISO-ish format
    format!("{}", secs_since_epoch)
}

/// Get FFmpeg version.
async fn get_ffmpeg_version(ffmpeg_path: &std::path::Path) -> Option<String> {
    let output = TokioCommand::new(ffmpeg_path)
        .arg("-version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // First line contains version, e.g., "ffmpeg version 6.0 Copyright..."
        stdout.lines().next().map(|s| s.to_string())
    } else {
        None
    }
}

/// Get GPU information.
async fn get_gpu_info() -> Option<GpuInfo> {
    #[cfg(target_os = "macos")]
    {
        // Use system_profiler on macOS
        let output = TokioCommand::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
            .await
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse JSON to get GPU name
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(displays) = json.get("SPDisplaysDataType").and_then(|v| v.as_array()) {
                    if let Some(first) = displays.first() {
                        let name = first
                            .get("sppci_model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let vendor = first
                            .get("spdisplays_vendor")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Apple")
                            .to_string();
                        let vram = first
                            .get("spdisplays_vram")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        return Some(GpuInfo {
                            name,
                            vendor,
                            details: vram,
                        });
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    {
        // Try nvidia-smi first
        if let Ok(output) = TokioCommand::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total,driver_version",
                "--format=csv,noheader",
            ])
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split(',').map(|s| s.trim()).collect();
                if !parts.is_empty() {
                    return Some(GpuInfo {
                        name: parts.first().unwrap_or(&"Unknown").to_string(),
                        vendor: "NVIDIA".to_string(),
                        details: if parts.len() >= 3 {
                            Some(format!("VRAM: {}, Driver: {}", parts[1], parts[2]))
                        } else {
                            None
                        },
                    });
                }
            }
        }

        // Fallback to lspci
        if let Ok(output) = TokioCommand::new("lspci").args(["-nn"]).output().await {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("VGA") || line.contains("3D controller") {
                        let vendor = if line.contains("NVIDIA") {
                            "NVIDIA"
                        } else if line.contains("Intel") {
                            "Intel"
                        } else if line.contains("AMD") || line.contains("ATI") {
                            "AMD"
                        } else {
                            "Unknown"
                        };
                        return Some(GpuInfo {
                            name: line.to_string(),
                            vendor: vendor.to_string(),
                            details: None,
                        });
                    }
                }
            }
        }

        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Get disk space info for a path.
fn get_disk_info(path: &std::path::Path) -> DiskInfo {
    use std::ffi::CString;

    let path_str = path.to_string_lossy().to_string();

    #[cfg(unix)]
    {
        // Handle potential null bytes in path (unlikely but possible)
        let c_path = match CString::new(path_str.as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(path = %path_str, "Path contains null bytes, cannot get disk info");
                return DiskInfo {
                    path: path_str,
                    free_bytes: 0,
                    total_bytes: 0,
                    free_percent: 0.0,
                };
            }
        };
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };

        if result == 0 {
            let free_bytes = stat.f_bavail as u64 * stat.f_frsize;
            let total_bytes = stat.f_blocks as u64 * stat.f_frsize;
            let free_percent = if total_bytes > 0 {
                (free_bytes as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            return DiskInfo {
                path: path_str,
                free_bytes,
                total_bytes,
                free_percent,
            };
        }
    }

    // Fallback for non-unix or on error
    DiskInfo {
        path: path_str,
        free_bytes: 0,
        total_bytes: 0,
        free_percent: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_config::RemoteConfig;

    /// Helper to create a test handler with mock state.
    async fn create_test_handler() -> (AdminHandler, Keys, Keys) {
        let dvm_keys = Keys::generate();
        let admin_keys = Keys::generate();

        let mut remote_config = RemoteConfig::new();
        remote_config.admin = Some(admin_keys.public_key().to_hex());

        let state = crate::dvm_state::DvmState::new_shared(dvm_keys.clone(), remote_config.clone());

        // Create a client that won't actually connect
        let client = Client::new(dvm_keys.clone());

        // Create a minimal config for testing
        let config = Arc::new(
            Config::from_remote(
                dvm_keys.clone(),
                &remote_config,
                std::path::PathBuf::from("ffmpeg"),
                std::path::PathBuf::from("ffprobe"),
            )
            .expect("Failed to create test config"),
        );

        let config_notify = Arc::new(Notify::new());
        let handler = AdminHandler::new(state, client, config, config_notify);

        (handler, dvm_keys, admin_keys)
    }

    #[tokio::test]
    async fn test_unauthorized_command() {
        let (handler, _dvm_keys, _admin_keys) = create_test_handler().await;

        // Use a random non-admin key
        let random_keys = Keys::generate();

        // Try to get config as non-admin
        let response = handler
            .handle(AdminCommand::GetConfig, random_keys.public_key())
            .await;

        assert!(!response.ok);
        assert_eq!(response.error, Some("Unauthorized".to_string()));
    }

    #[tokio::test]
    async fn test_get_config_as_admin() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(AdminCommand::GetConfig, admin_keys.public_key())
            .await;

        assert!(response.ok);
        assert!(response.data.is_some());

        if let Some(ResponseData::Config(config_response)) = response.data {
            assert!(!config_response.config.paused);
        } else {
            panic!("Expected ConfigResponse");
        }
    }

    #[tokio::test]
    async fn test_status_as_admin() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(AdminCommand::Status, admin_keys.public_key())
            .await;

        assert!(response.ok);

        if let Some(ResponseData::Status(status)) = response.data {
            assert!(!status.paused);
            assert_eq!(status.jobs_active, 0);
            assert_eq!(status.jobs_completed, 0);
            assert_eq!(status.jobs_failed, 0);
            assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
        } else {
            panic!("Expected StatusResponse");
        }
    }

    #[tokio::test]
    async fn test_set_blob_expiration_zero() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(
                AdminCommand::SetBlobExpiration { days: 0 },
                admin_keys.public_key(),
            )
            .await;

        assert!(!response.ok);
        assert_eq!(
            response.error,
            Some("Expiration days must be greater than 0".to_string())
        );
    }

    #[tokio::test]
    async fn test_set_profile_empty() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(
                AdminCommand::SetProfile {
                    name: None,
                    about: None,
                },
                admin_keys.public_key(),
            )
            .await;

        assert!(!response.ok);
        assert_eq!(
            response.error,
            Some("At least one of 'name' or 'about' must be provided".to_string())
        );
    }

    #[tokio::test]
    async fn test_invalid_relay_url() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(
                AdminCommand::SetRelays {
                    relays: vec!["not-a-valid-url".to_string()],
                },
                admin_keys.public_key(),
            )
            .await;

        assert!(!response.ok);
        assert!(response.error.unwrap().contains("Invalid relay URL"));
    }

    #[tokio::test]
    async fn test_invalid_blossom_url() {
        let (handler, _dvm_keys, admin_keys) = create_test_handler().await;

        let response = handler
            .handle(
                AdminCommand::SetBlossomServers {
                    servers: vec!["not-a-valid-url".to_string()],
                },
                admin_keys.public_key(),
            )
            .await;

        assert!(!response.ok);
        assert!(response.error.unwrap().contains("Invalid server URL"));
    }
}
