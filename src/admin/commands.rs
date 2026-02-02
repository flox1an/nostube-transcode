//! Admin command types and parsing.
//!
//! This module defines the command and response types for admin DM interactions.

use serde::{Deserialize, Serialize};

/// Admin commands received via encrypted DMs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AdminCommand {
    /// Claim admin role using the pairing secret
    ClaimAdmin { secret: String },
    /// Get current configuration
    GetConfig,
    /// Update relay list
    SetRelays { relays: Vec<String> },
    /// Update Blossom server list
    SetBlossomServers { servers: Vec<String> },
    /// Update blob expiration period
    SetBlobExpiration { days: u32 },
    /// Update DVM profile (name and/or about)
    SetProfile {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        about: Option<String>,
    },
    /// Pause the DVM (reject new jobs)
    Pause,
    /// Resume the DVM (accept new jobs)
    Resume,
    /// Get current status
    Status,
    /// Get job history
    JobHistory {
        #[serde(default = "default_job_history_limit")]
        limit: u32,
    },
    /// Run self-test (encode a short video)
    SelfTest,
    /// Get system information (hardware, GPU, disk, FFmpeg)
    SystemInfo,
    /// Import configuration from environment variables
    ImportEnvConfig,
}

fn default_job_history_limit() -> u32 {
    20
}

/// Response to admin commands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdminResponse {
    /// Whether the command succeeded
    pub ok: bool,
    /// Error message if command failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Success message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    /// Response data (flattened into response object)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

impl AdminResponse {
    /// Create a success response with no data.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            msg: None,
            data: None,
        }
    }

    /// Create a success response with a message.
    pub fn ok_with_msg(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            error: None,
            msg: Some(msg.into()),
            data: None,
        }
    }

    /// Create a success response with data.
    pub fn ok_with_data(data: ResponseData) -> Self {
        Self {
            ok: true,
            error: None,
            msg: None,
            data: Some(data),
        }
    }

    /// Create an error response.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            msg: None,
            data: None,
        }
    }
}

/// Response data types (untagged for cleaner JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ResponseData {
    /// Configuration data
    Config(ConfigResponse),
    /// Status data
    Status(StatusResponse),
    /// Job history data
    JobHistory(JobHistoryResponse),
    /// Self-test results
    SelfTest(SelfTestResponse),
    /// System information
    SystemInfo(SystemInfoResponse),
}

/// Configuration response data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigResponse {
    /// The current configuration
    pub config: ConfigData,
}

/// Configuration data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigData {
    /// Nostr relays
    pub relays: Vec<String>,
    /// Blossom upload servers
    pub blossom_servers: Vec<String>,
    /// Blob expiration in days
    pub blob_expiration_days: u32,
    /// DVM display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// DVM description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// Whether DVM is paused
    pub paused: bool,
}

/// Status response data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusResponse {
    /// Whether DVM is paused
    pub paused: bool,
    /// Number of jobs currently active
    pub jobs_active: u32,
    /// Number of jobs completed successfully
    pub jobs_completed: u32,
    /// Number of jobs that failed
    pub jobs_failed: u32,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Hardware acceleration type in use
    pub hwaccel: String,
    /// DVM version
    pub version: String,
}

/// Job history response data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobHistoryResponse {
    /// List of recent jobs
    pub jobs: Vec<JobInfo>,
}

/// Information about a single job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobInfo {
    /// Job ID (event ID)
    pub id: String,
    /// Job status
    pub status: String,
    /// Input video URL
    pub input_url: String,
    /// Output HLS URL (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_url: Option<String>,
    /// When job started (ISO 8601)
    pub started_at: String,
    /// When job completed (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Processing duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

/// Self-test response data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfTestResponse {
    /// Whether the self-test passed
    pub success: bool,
    /// Duration of test video in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_duration_secs: Option<f64>,
    /// Encode time in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_time_secs: Option<f64>,
    /// Speed ratio (video duration / encode time)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_ratio: Option<f64>,
    /// Human-readable speed description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_description: Option<String>,
    /// Hardware acceleration used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwaccel: Option<String>,
    /// Resolution tested
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Output file size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_size_bytes: Option<u64>,
    /// Error message if test failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// System information response data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemInfoResponse {
    /// Platform (macos, linux, windows)
    pub platform: String,
    /// Architecture (x86_64, aarch64, etc.)
    pub arch: String,
    /// Available hardware encoders
    pub hw_encoders: Vec<HwEncoderInfo>,
    /// GPU information (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<GpuInfo>,
    /// Disk space information
    pub disk: DiskInfo,
    /// FFmpeg information
    pub ffmpeg: FfmpegInfo,
    /// Temp directory path
    pub temp_dir: String,
}

/// Hardware encoder info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HwEncoderInfo {
    /// Encoder name (e.g., "NVIDIA NVENC")
    pub name: String,
    /// Whether this is the currently selected encoder
    pub selected: bool,
    /// Supported codecs
    pub codecs: Vec<String>,
}

/// GPU information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuInfo {
    /// GPU name/model
    pub name: String,
    /// GPU vendor
    pub vendor: String,
    /// Additional details (driver version, VRAM, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Disk space information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskInfo {
    /// Path being checked
    pub path: String,
    /// Free space in bytes
    pub free_bytes: u64,
    /// Total space in bytes
    pub total_bytes: u64,
    /// Free space as percentage
    pub free_percent: f64,
}

/// FFmpeg information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FfmpegInfo {
    /// Path to FFmpeg binary
    pub path: String,
    /// FFmpeg version string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Path to FFprobe binary
    pub ffprobe_path: String,
}

/// Parse an admin command from JSON.
pub fn parse_command(json: &str) -> Result<AdminCommand, serde_json::Error> {
    serde_json::from_str(json)
}

/// Serialize an admin response to JSON.
pub fn serialize_response(response: &AdminResponse) -> Result<String, serde_json::Error> {
    serde_json::to_string(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_claim_admin() {
        let json = r#"{"cmd": "claim_admin", "secret": "abc1-def2-ghi3"}"#;
        let cmd = parse_command(json).unwrap();

        assert_eq!(
            cmd,
            AdminCommand::ClaimAdmin {
                secret: "abc1-def2-ghi3".to_string()
            }
        );
    }

    #[test]
    fn test_parse_get_config() {
        let json = r#"{"cmd": "get_config"}"#;
        let cmd = parse_command(json).unwrap();

        assert_eq!(cmd, AdminCommand::GetConfig);
    }

    #[test]
    fn test_parse_set_relays() {
        let json = r#"{"cmd": "set_relays", "relays": ["wss://relay1.example.com", "wss://relay2.example.com"]}"#;
        let cmd = parse_command(json).unwrap();

        assert_eq!(
            cmd,
            AdminCommand::SetRelays {
                relays: vec![
                    "wss://relay1.example.com".to_string(),
                    "wss://relay2.example.com".to_string()
                ]
            }
        );
    }

    #[test]
    fn test_parse_set_profile() {
        let json = r#"{"cmd": "set_profile", "name": "My DVM", "about": "A test DVM"}"#;
        let cmd = parse_command(json).unwrap();

        assert_eq!(
            cmd,
            AdminCommand::SetProfile {
                name: Some("My DVM".to_string()),
                about: Some("A test DVM".to_string())
            }
        );

        // Test with only name
        let json2 = r#"{"cmd": "set_profile", "name": "My DVM"}"#;
        let cmd2 = parse_command(json2).unwrap();

        assert_eq!(
            cmd2,
            AdminCommand::SetProfile {
                name: Some("My DVM".to_string()),
                about: None
            }
        );
    }

    #[test]
    fn test_parse_job_history_default_limit() {
        let json = r#"{"cmd": "job_history"}"#;
        let cmd = parse_command(json).unwrap();

        assert_eq!(cmd, AdminCommand::JobHistory { limit: 20 });

        // Test with explicit limit
        let json2 = r#"{"cmd": "job_history", "limit": 50}"#;
        let cmd2 = parse_command(json2).unwrap();

        assert_eq!(cmd2, AdminCommand::JobHistory { limit: 50 });
    }

    #[test]
    fn test_serialize_ok_response() {
        let response = AdminResponse::ok();
        let json = serialize_response(&response).unwrap();

        assert_eq!(json, r#"{"ok":true}"#);

        // Test with message
        let response_msg = AdminResponse::ok_with_msg("Config updated");
        let json_msg = serialize_response(&response_msg).unwrap();

        assert_eq!(json_msg, r#"{"ok":true,"msg":"Config updated"}"#);
    }

    #[test]
    fn test_serialize_error_response() {
        let response = AdminResponse::error("Invalid secret");
        let json = serialize_response(&response).unwrap();

        assert_eq!(json, r#"{"ok":false,"error":"Invalid secret"}"#);
    }

    #[test]
    fn test_serialize_config_response() {
        let config_data = ConfigData {
            relays: vec!["wss://relay.example.com".to_string()],
            blossom_servers: vec!["https://blossom.example.com".to_string()],
            blob_expiration_days: 30,
            name: Some("Test DVM".to_string()),
            about: None,
            paused: false,
        };

        let response = AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse {
            config: config_data,
        }));

        let json = serialize_response(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["config"]["relays"][0], "wss://relay.example.com");
        assert_eq!(
            parsed["config"]["blossom_servers"][0],
            "https://blossom.example.com"
        );
        assert_eq!(parsed["config"]["blob_expiration_days"], 30);
        assert_eq!(parsed["config"]["name"], "Test DVM");
        assert!(parsed["config"]["about"].is_null());
        assert_eq!(parsed["config"]["paused"], false);
    }
}
