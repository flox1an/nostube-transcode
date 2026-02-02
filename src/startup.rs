//! DVM startup orchestration.
//!
//! Handles the complete startup sequence including identity loading,
//! config fetching, and pairing mode.

use crate::bootstrap::{get_admin_app_urls, get_bootstrap_relays};
use crate::config::Config;
use crate::dvm_state::{DvmState, SharedDvmState};
use crate::identity::load_or_generate_identity;
use crate::pairing::PairingState;
use crate::remote_config::{fetch_config, RemoteConfig};
use crate::util::ffmpeg_discovery::FfmpegPaths;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Result of startup initialization
pub struct StartupResult {
    pub keys: Keys,
    pub client: Client,
    pub state: SharedDvmState,
    pub pairing: Arc<RwLock<Option<PairingState>>>,
    pub needs_pairing: bool,
    pub config: Arc<Config>,
}

/// Initialize the DVM on startup.
///
/// 1. Load or generate identity
/// 2. Connect to bootstrap relays
/// 3. Fetch remote config (if exists)
/// 4. Create DVM state
/// 5. Enter pairing mode if no admin configured
pub async fn initialize() -> Result<StartupResult, Box<dyn std::error::Error>> {
    // Step 1: Load or generate identity
    tracing::info!("Loading identity...");
    let keys = load_or_generate_identity()?;
    let npub = keys.public_key().to_bech32().unwrap_or_default();
    tracing::info!("DVM pubkey: {}", npub);

    // Step 2: Connect to bootstrap relays
    tracing::info!("Connecting to bootstrap relays...");
    let client = Client::new(keys.clone());

    for relay in get_bootstrap_relays() {
        if let Err(e) = client.add_relay(relay.to_string()).await {
            tracing::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;

    // Step 3: Fetch remote config
    tracing::info!("Fetching remote configuration...");
    let remote_config = match fetch_config(&client, &keys).await {
        Ok(Some(config)) => {
            tracing::info!("Loaded remote config (version {})", config.version);
            config
        }
        Ok(None) => {
            tracing::info!("No remote config found, using defaults");
            RemoteConfig::new()
        }
        Err(e) => {
            tracing::warn!("Failed to fetch config: {}, using defaults", e);
            RemoteConfig::new()
        }
    };

    // Step 4: Check if we need pairing
    let needs_pairing = !remote_config.has_admin();
    let pairing = Arc::new(RwLock::new(None));

    if needs_pairing {
        tracing::info!("No admin configured, entering pairing mode");

        // Create pairing state and display
        let pairing_state = PairingState::new(keys.public_key());
        pairing_state.display_urls(&get_admin_app_urls());

        *pairing.write().await = Some(pairing_state);
    } else {
        tracing::info!(
            "Admin configured: {}",
            remote_config.admin.as_deref().unwrap_or("unknown")
        );

        // Connect to configured relays (in addition to bootstrap)
        if !remote_config.relays.is_empty() {
            tracing::info!("Adding configured relays...");
            for relay in &remote_config.relays {
                if let Err(e) = client.add_relay(relay.clone()).await {
                    tracing::warn!("Failed to add relay {}: {}", relay, e);
                }
            }
        }
    }

    // Step 5: Discover FFmpeg binaries
    tracing::info!("Discovering FFmpeg binaries...");
    let ffmpeg_paths = FfmpegPaths::discover()?;

    // Step 6: Create Config from RemoteConfig
    let config = Arc::new(Config::from_remote(
        keys.clone(),
        &remote_config,
        ffmpeg_paths.ffmpeg,
        ffmpeg_paths.ffprobe,
    )?);

    // Step 7: Create DVM state
    let state = DvmState::new_shared(keys.clone(), remote_config);

    Ok(StartupResult {
        keys,
        client,
        state,
        pairing,
        needs_pairing,
        config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup_result_fields() {
        // Verify the struct can be created manually
        let keys = Keys::generate();
        let client = Client::new(keys.clone());
        let state = DvmState::new_shared(keys.clone(), RemoteConfig::new());
        let pairing = Arc::new(RwLock::new(None));
        let config = Arc::new(Config::from_remote(
            keys.clone(),
            &RemoteConfig::new(),
            std::path::PathBuf::from("ffmpeg"),
            std::path::PathBuf::from("ffprobe"),
        ).unwrap());

        let result = StartupResult {
            keys: keys.clone(),
            client,
            state,
            pairing,
            needs_pairing: true,
            config,
        };

        assert!(result.needs_pairing);
        assert_eq!(result.keys.public_key(), keys.public_key());
    }
}
