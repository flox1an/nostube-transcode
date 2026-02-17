//! DVM startup orchestration.
//!
//! Handles the complete startup sequence including identity loading
//! and config fetching.

use crate::bootstrap::get_bootstrap_relays;
use crate::config::Config;
use crate::dvm_state::{DvmState, SharedDvmState};
use crate::identity::load_or_generate_identity;
use crate::remote_config::{fetch_config, RemoteConfig};
use crate::util::ffmpeg_discovery::FfmpegPaths;
use nostr_sdk::prelude::*;
use std::sync::Arc;

/// Result of startup initialization
pub struct StartupResult {
    pub keys: Keys,
    pub client: Client,
    pub state: SharedDvmState,
    pub config: Arc<Config>,
}

/// Initialize the DVM on startup.
///
/// 1. Load or generate identity
/// 2. Read OPERATOR_NPUB (required)
/// 3. Connect to bootstrap relays
/// 4. Fetch remote config (if exists)
/// 5. Set admin from OPERATOR_NPUB if not already in remote config
/// 6. Create DVM state
pub async fn initialize() -> Result<StartupResult, Box<dyn std::error::Error>> {
    // Step 1: Load or generate identity
    tracing::info!("Loading identity...");
    let keys = load_or_generate_identity()?;
    let npub = keys.public_key().to_bech32().unwrap_or_default();
    tracing::info!("DVM pubkey: {}", npub);

    // Step 2: Read and validate OPERATOR_NPUB
    let operator_npub = std::env::var("OPERATOR_NPUB").unwrap_or_else(|_| {
        eprintln!("ERROR: OPERATOR_NPUB environment variable is required.");
        eprintln!("Set it to the npub or hex pubkey of the DVM operator.");
        eprintln!("Example: OPERATOR_NPUB=npub1... cargo run");
        std::process::exit(1);
    });

    let operator_pubkey = PublicKey::parse(&operator_npub).unwrap_or_else(|e| {
        eprintln!(
            "ERROR: Invalid OPERATOR_NPUB '{}': {}",
            operator_npub, e
        );
        eprintln!("Must be a valid npub (npub1...) or hex public key.");
        std::process::exit(1);
    });

    tracing::info!(
        "Operator pubkey: {}",
        operator_pubkey.to_bech32().unwrap_or_default()
    );

    // Step 3: Connect to bootstrap relays
    tracing::info!("Connecting to bootstrap relays...");
    let client = Client::new(keys.clone());

    for relay in get_bootstrap_relays() {
        if let Err(e) = client.add_relay(relay.to_string()).await {
            tracing::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;

    // Step 4: Fetch remote config
    tracing::info!("Fetching remote configuration...");
    let mut remote_config = match fetch_config(&client, &keys).await {
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

    // Step 5: Ensure admin is set from OPERATOR_NPUB
    if !remote_config.has_admin() {
        remote_config.admin = Some(operator_pubkey.to_hex());
    }

    // Seed bootstrap relays into config when no relays are configured
    if remote_config.relays.is_empty() {
        remote_config.relays = get_bootstrap_relays()
            .into_iter()
            .map(|u| u.to_string())
            .collect();
        tracing::info!("Using bootstrap relays as default config relays");
    }

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
        client.connect().await;
    }

    // Step 6: Discover FFmpeg binaries
    tracing::info!("Discovering FFmpeg binaries...");
    let ffmpeg_paths = FfmpegPaths::discover()?;

    // Step 7: Create Config from RemoteConfig
    let config = Arc::new(Config::from_remote(
        keys.clone(),
        &remote_config,
        ffmpeg_paths.ffmpeg,
        ffmpeg_paths.ffprobe,
    )?);

    // Step 8: Create DVM state
    let state = DvmState::new_shared(keys.clone(), remote_config);

    Ok(StartupResult {
        keys,
        client,
        state,
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
        let config = Arc::new(
            Config::from_remote(
                keys.clone(),
                &RemoteConfig::new(),
                std::path::PathBuf::from("ffmpeg"),
                std::path::PathBuf::from("ffprobe"),
            )
            .unwrap(),
        );

        let result = StartupResult {
            keys: keys.clone(),
            client,
            state,
            config,
        };

        assert_eq!(result.keys.public_key(), keys.public_key());
    }
}
