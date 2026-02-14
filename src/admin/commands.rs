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
    /// Get dashboard data (status + config + recent jobs) in one response
    GetDashboard {
        #[serde(default = "default_job_history_limit")]
        limit: u32,
    },
    /// Update configuration (all fields optional, returns updated config)
    SetConfig {
        #[serde(skip_serializing_if = "Option::is_none")]
        relays: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blossom_servers: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blob_expiration_days: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        about: Option<String>,
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

/// Wire format for incoming admin requests (NIP-46-style RPC).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdminRequest {
    /// Unique request identifier
    pub id: String,
    /// Method name (maps to AdminCommand variant)
    pub method: String,
    /// Method parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

impl AdminRequest {
    /// Convert this wire-format request into an internal `AdminCommand`.
    pub fn to_command(&self) -> Result<AdminCommand, String> {
        match self.method.as_str() {
            "claim_admin" => {
                let secret = self.params.get("secret")
                    .and_then(|v| v.as_str())
                    .ok_or("claim_admin requires 'secret' param")?
                    .to_string();
                Ok(AdminCommand::ClaimAdmin { secret })
            }
            "get_config" => Ok(AdminCommand::GetConfig),
            "set_relays" => {
                let relays = self.params.get("relays")
                    .ok_or("set_relays requires 'relays' param")?;
                let relays: Vec<String> = serde_json::from_value(relays.clone())
                    .map_err(|e| format!("invalid relays: {e}"))?;
                Ok(AdminCommand::SetRelays { relays })
            }
            "set_blossom_servers" => {
                let servers = self.params.get("servers")
                    .ok_or("set_blossom_servers requires 'servers' param")?;
                let servers: Vec<String> = serde_json::from_value(servers.clone())
                    .map_err(|e| format!("invalid servers: {e}"))?;
                Ok(AdminCommand::SetBlossomServers { servers })
            }
            "set_blob_expiration" => {
                let days = self.params.get("days")
                    .ok_or("set_blob_expiration requires 'days' param")?;
                let days: u32 = serde_json::from_value(days.clone())
                    .map_err(|e| format!("invalid days: {e}"))?;
                Ok(AdminCommand::SetBlobExpiration { days })
            }
            "set_profile" => {
                let name = self.params.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let about = self.params.get("about")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Ok(AdminCommand::SetProfile { name, about })
            }
            "pause" => Ok(AdminCommand::Pause),
            "resume" => Ok(AdminCommand::Resume),
            "status" => Ok(AdminCommand::Status),
            "job_history" => {
                let limit = self.params.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(20);
                Ok(AdminCommand::JobHistory { limit })
            }
            "get_dashboard" => {
                let limit = self.params.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(20);
                Ok(AdminCommand::GetDashboard { limit })
            }
            "set_config" => {
                let relays = self.params.get("relays")
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| format!("invalid relays: {e}"))?;
                let blossom_servers = self.params.get("blossom_servers")
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| format!("invalid blossom_servers: {e}"))?;
                let blob_expiration_days = self.params.get("blob_expiration_days")
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| format!("invalid blob_expiration_days: {e}"))?;
                let name = self.params.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let about = self.params.get("about")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Ok(AdminCommand::SetConfig {
                    relays,
                    blossom_servers,
                    blob_expiration_days,
                    name,
                    about,
                })
            }
            "self_test" => Ok(AdminCommand::SelfTest),
            "system_info" => Ok(AdminCommand::SystemInfo),
            "import_env_config" => Ok(AdminCommand::ImportEnvConfig),
            _ => Err(format!("unknown method: {}", self.method)),
        }
    }
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

/// Wire format for outgoing admin responses (NIP-46-style RPC).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdminResponseWire {
    /// Request identifier this response corresponds to
    pub id: String,
    /// Result data on success
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error message on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AdminResponseWire {
    /// Convert an internal `AdminResponse` into wire format.
    pub fn from_response(id: String, response: AdminResponse) -> Self {
        if !response.ok {
            return Self {
                id,
                result: None,
                error: response.error,
            };
        }

        if let Some(data) = response.data {
            return Self {
                id,
                result: serde_json::to_value(data).ok(),
                error: None,
            };
        }

        if let Some(msg) = response.msg {
            return Self {
                id,
                result: Some(serde_json::json!({ "msg": msg })),
                error: None,
            };
        }

        Self {
            id,
            result: Some(serde_json::json!({})),
            error: None,
        }
    }
}

/// Response data types (untagged for cleaner JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ResponseData {
    /// Dashboard data (status + config + jobs)
    Dashboard(DashboardResponse),
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

/// Dashboard response data (status + config + jobs combined).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardResponse {
    /// Current status
    pub status: StatusResponse,
    /// Current configuration
    pub config: ConfigData,
    /// Recent jobs
    pub jobs: Vec<JobInfo>,
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

/// Parse an admin request from JSON (protocol v2 wire format).
pub fn parse_request(json: &str) -> Result<AdminRequest, serde_json::Error> {
    serde_json::from_str(json)
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

    // --- AdminRequest / AdminResponseWire tests (protocol v2) ---

    #[test]
    fn test_parse_request_get_config() {
        let json = r#"{"id":"req-1","method":"get_config"}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.id, "req-1");
        assert_eq!(req.method, "get_config");
        let cmd = req.to_command().unwrap();
        assert_eq!(cmd, AdminCommand::GetConfig);
    }

    #[test]
    fn test_parse_request_claim_admin() {
        let json = r#"{"id":"req-2","method":"claim_admin","params":{"secret":"abc-123"}}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(
            cmd,
            AdminCommand::ClaimAdmin {
                secret: "abc-123".to_string()
            }
        );
    }

    #[test]
    fn test_parse_request_set_relays() {
        let json = r#"{"id":"req-3","method":"set_relays","params":{"relays":["wss://r1.example.com","wss://r2.example.com"]}}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(
            cmd,
            AdminCommand::SetRelays {
                relays: vec![
                    "wss://r1.example.com".to_string(),
                    "wss://r2.example.com".to_string()
                ]
            }
        );
    }

    #[test]
    fn test_parse_request_set_profile() {
        let json =
            r#"{"id":"req-4","method":"set_profile","params":{"name":"My DVM","about":"desc"}}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(
            cmd,
            AdminCommand::SetProfile {
                name: Some("My DVM".to_string()),
                about: Some("desc".to_string())
            }
        );
    }

    #[test]
    fn test_parse_request_job_history_default() {
        let json = r#"{"id":"req-5","method":"job_history"}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(cmd, AdminCommand::JobHistory { limit: 20 });
    }

    #[test]
    fn test_parse_request_job_history_explicit() {
        let json = r#"{"id":"req-6","method":"job_history","params":{"limit":50}}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(cmd, AdminCommand::JobHistory { limit: 50 });
    }

    #[test]
    fn test_parse_request_set_config() {
        let json = r#"{"id":"req-7","method":"set_config","params":{"relays":["wss://r.example.com"],"name":"Updated"}}"#;
        let req = parse_request(json).unwrap();
        let cmd = req.to_command().unwrap();
        assert_eq!(
            cmd,
            AdminCommand::SetConfig {
                relays: Some(vec!["wss://r.example.com".to_string()]),
                blossom_servers: None,
                blob_expiration_days: None,
                name: Some("Updated".to_string()),
                about: None,
            }
        );
    }

    #[test]
    fn test_parse_request_unknown_method() {
        let json = r#"{"id":"req-8","method":"fly_to_moon"}"#;
        let req = parse_request(json).unwrap();
        let result = req.to_command();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown method"));
    }

    #[test]
    fn test_response_wire_success_with_msg() {
        let response = AdminResponse::ok_with_msg("Config updated");
        let wire = AdminResponseWire::from_response("req-1".to_string(), response);
        assert_eq!(wire.id, "req-1");
        assert!(wire.error.is_none());
        let result = wire.result.unwrap();
        assert_eq!(result["msg"], "Config updated");
    }

    #[test]
    fn test_response_wire_success_with_data() {
        let config_data = ConfigData {
            relays: vec!["wss://relay.example.com".to_string()],
            blossom_servers: vec![],
            blob_expiration_days: 30,
            name: None,
            about: None,
            paused: false,
        };
        let response = AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse {
            config: config_data,
        }));
        let wire = AdminResponseWire::from_response("req-2".to_string(), response);
        assert_eq!(wire.id, "req-2");
        assert!(wire.error.is_none());
        let result = wire.result.unwrap();
        assert_eq!(result["config"]["relays"][0], "wss://relay.example.com");
    }

    #[test]
    fn test_response_wire_success_empty() {
        let response = AdminResponse::ok();
        let wire = AdminResponseWire::from_response("req-3".to_string(), response);
        assert_eq!(wire.id, "req-3");
        assert!(wire.error.is_none());
        let result = wire.result.unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[test]
    fn test_response_wire_error() {
        let response = AdminResponse::error("something went wrong");
        let wire = AdminResponseWire::from_response("req-4".to_string(), response);
        assert_eq!(wire.id, "req-4");
        assert!(wire.result.is_none());
        assert_eq!(wire.error.unwrap(), "something went wrong");
    }

    #[test]
    fn test_response_wire_serialization_skips_none() {
        let wire = AdminResponseWire {
            id: "req-5".to_string(),
            result: Some(serde_json::json!({"msg": "ok"})),
            error: None,
        };
        let json = serde_json::to_string(&wire).unwrap();
        assert!(!json.contains("error"));

        let wire_err = AdminResponseWire {
            id: "req-6".to_string(),
            result: None,
            error: Some("fail".to_string()),
        };
        let json_err = serde_json::to_string(&wire_err).unwrap();
        assert!(!json_err.contains("result"));
    }
}
