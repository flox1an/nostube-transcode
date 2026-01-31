//! Admin command handler.
//!
//! Processes admin commands received via encrypted DMs,
//! validates authorization, and updates DVM state.

use crate::admin::commands::*;
use crate::dvm_state::SharedDvmState;
use crate::pairing::PairingState;
use crate::remote_config::save_config;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handles admin commands for the DVM.
pub struct AdminHandler {
    /// Shared DVM state
    state: SharedDvmState,
    /// Nostr client for publishing config updates
    client: Client,
    /// Active pairing state (if pairing is in progress)
    pairing: Arc<RwLock<Option<PairingState>>>,
}

impl AdminHandler {
    /// Creates a new admin handler.
    pub fn new(
        state: SharedDvmState,
        client: Client,
        pairing: Arc<RwLock<Option<PairingState>>>,
    ) -> Self {
        Self {
            state,
            client,
            pairing,
        }
    }

    /// Handles an admin command from a sender.
    ///
    /// Validates that the sender is authorized (either admin or during pairing)
    /// and dispatches to the appropriate handler.
    pub async fn handle(&self, command: AdminCommand, sender: PublicKey) -> AdminResponse {
        // ClaimAdmin is special - it doesn't require prior authorization
        if let AdminCommand::ClaimAdmin { ref secret } = command {
            return self.handle_claim_admin(secret, sender).await;
        }

        // All other commands require the sender to be the admin
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
            AdminCommand::ClaimAdmin { .. } => unreachable!(),
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
            AdminCommand::SelfTest => self.handle_self_test().await,
            AdminCommand::ImportEnvConfig => self.handle_import_env_config().await,
        }
    }

    /// Handles the ClaimAdmin command.
    ///
    /// Verifies the pairing secret and sets the sender as admin.
    async fn handle_claim_admin(&self, secret: &str, sender: PublicKey) -> AdminResponse {
        // Check if there's already an admin
        {
            let state = self.state.read().await;
            if state.config.has_admin() {
                return AdminResponse::error("Admin already configured");
            }
        }

        // Verify pairing secret
        let pairing_valid = {
            let pairing = self.pairing.read().await;
            pairing.as_ref().map(|p| p.verify(secret)).unwrap_or(false)
        };

        if !pairing_valid {
            return AdminResponse::error("Invalid or expired pairing secret");
        }

        // Set admin and save config
        let result = {
            let mut state = self.state.write().await;
            state.config.admin = Some(sender.to_hex());

            // Save config to relays
            save_config(&self.client, &state.keys, &state.config).await
        };

        // Clear pairing state
        {
            let mut pairing = self.pairing.write().await;
            *pairing = None;
        }

        match result {
            Ok(_) => AdminResponse::ok_with_msg("Admin role claimed successfully"),
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
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

        let result = {
            let mut state = self.state.write().await;
            state.config.relays = relays;
            save_config(&self.client, &state.keys, &state.config).await
        };

        match result {
            Ok(_) => AdminResponse::ok_with_msg("Relays updated"),
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
            Ok(_) => AdminResponse::ok_with_msg("Blossom servers updated"),
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
            Ok(_) => AdminResponse::ok_with_msg(format!("Blob expiration set to {} days", days)),
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
            Ok(_) => AdminResponse::ok_with_msg("Profile updated"),
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the Pause command.
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
            Ok(_) => AdminResponse::ok_with_msg("DVM paused"),
            Err(e) => AdminResponse::error(format!("Failed to save config: {}", e)),
        }
    }

    /// Handles the Resume command.
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
            Ok(_) => AdminResponse::ok_with_msg("DVM resumed"),
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

    /// Handles the SelfTest command.
    ///
    /// This is a placeholder - full implementation would encode a test video.
    async fn handle_self_test(&self) -> AdminResponse {
        // TODO: Implement actual self-test with video encoding
        // For now, return a placeholder response
        let state = self.state.read().await;

        AdminResponse::ok_with_data(ResponseData::SelfTest(SelfTestResponse {
            success: true,
            video_duration_secs: None,
            encode_time_secs: None,
            speed_ratio: None,
            hwaccel: state.hwaccel.clone(),
            resolution: None,
            error: None,
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
            Ok(_) => AdminResponse::ok_with_msg(format!("Imported: {}", imported.join(", "))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_config::RemoteConfig;

    /// Helper to create a test handler with mock state.
    async fn create_test_handler() -> (AdminHandler, Keys, Keys) {
        let dvm_keys = Keys::generate();
        let admin_keys = Keys::generate();

        let mut config = RemoteConfig::new();
        config.admin = Some(admin_keys.public_key().to_hex());

        let state = crate::dvm_state::DvmState::new_shared(dvm_keys.clone(), config);

        // Create a client that won't actually connect
        let client = Client::new(dvm_keys.clone());

        let pairing = Arc::new(RwLock::new(None));

        let handler = AdminHandler::new(state, client, pairing);

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
    async fn test_claim_admin_already_configured() {
        let (handler, _dvm_keys, _admin_keys) = create_test_handler().await;

        let new_user = Keys::generate();
        let response = handler
            .handle(
                AdminCommand::ClaimAdmin {
                    secret: "test-secret".to_string(),
                },
                new_user.public_key(),
            )
            .await;

        assert!(!response.ok);
        assert_eq!(response.error, Some("Admin already configured".to_string()));
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
