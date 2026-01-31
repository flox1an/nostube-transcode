//! Remote configuration storage and retrieval via NIP-78.
//!
//! Configuration is stored as an encrypted kind 30078 event on Nostr relays.
//! Only the DVM can decrypt its own config.

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// NIP-78 application-specific data kind
pub const KIND_APP_SPECIFIC_DATA: Kind = Kind::Custom(30078);

/// The d-tag identifier for DVM config
pub const CONFIG_D_TAG: &str = "video-dvm-config";

#[derive(Error, Debug)]
pub enum RemoteConfigError {
    #[error("Config not found on relays")]
    NotFound,
    #[error("Failed to decrypt config: {0}")]
    DecryptionError(String),
    #[error("Invalid config format: {0}")]
    InvalidFormat(#[from] serde_json::Error),
    #[error("Relay error: {0}")]
    RelayError(String),
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}

/// Schema version for forward compatibility
pub const CONFIG_VERSION: u32 = 1;

/// Remote configuration stored on Nostr
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteConfig {
    /// Schema version
    pub version: u32,
    /// Admin pubkey (npub or hex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<String>,
    /// Nostr relays for DVM operation
    #[serde(default)]
    pub relays: Vec<String>,
    /// Blossom upload servers
    #[serde(default)]
    pub blossom_servers: Vec<String>,
    /// Blob expiration in days
    #[serde(default = "default_expiration")]
    pub blob_expiration_days: u32,
    /// DVM display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// DVM description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// Whether DVM is paused (rejecting new jobs)
    #[serde(default)]
    pub paused: bool,
}

fn default_expiration() -> u32 {
    30
}

impl RemoteConfig {
    /// Create a new empty config with defaults
    pub fn new() -> Self {
        Self {
            version: CONFIG_VERSION,
            blob_expiration_days: 30,
            ..Default::default()
        }
    }

    /// Check if this config has an admin configured
    pub fn has_admin(&self) -> bool {
        self.admin.is_some()
    }

    /// Parse the admin pubkey if present
    pub fn admin_pubkey(&self) -> Option<PublicKey> {
        self.admin.as_ref().and_then(|s| PublicKey::parse(s).ok())
    }
}

/// Fetches the DVM's remote config from relays.
///
/// Queries for kind 30078 events with d-tag "video-dvm-config" authored by the DVM.
/// Decrypts using NIP-44.
pub async fn fetch_config(
    client: &Client,
    keys: &Keys,
) -> Result<Option<RemoteConfig>, RemoteConfigError> {
    let filter = Filter::new()
        .kind(KIND_APP_SPECIFIC_DATA)
        .author(keys.public_key())
        .custom_tag(SingleLetterTag::lowercase(Alphabet::D), [CONFIG_D_TAG])
        .limit(1);

    let events = client
        .get_events_of(vec![filter], EventSource::relays(Some(Duration::from_secs(10))))
        .await
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?;

    let event = match events.into_iter().next() {
        Some(e) => e,
        None => return Ok(None),
    };

    // Decrypt content using NIP-44 (encrypted to self)
    let decrypted = nip44::decrypt(keys.secret_key(), &keys.public_key(), &event.content)
        .map_err(|e| RemoteConfigError::DecryptionError(e.to_string()))?;

    let config: RemoteConfig = serde_json::from_str(&decrypted)?;

    Ok(Some(config))
}

/// Saves the DVM's remote config to relays.
///
/// Creates a kind 30078 event with d-tag "video-dvm-config".
/// Content is NIP-44 encrypted to self.
pub async fn save_config(
    client: &Client,
    keys: &Keys,
    config: &RemoteConfig,
) -> Result<EventId, RemoteConfigError> {
    let json = serde_json::to_string(config)?;

    // Encrypt to self using NIP-44
    let encrypted = nip44::encrypt(
        keys.secret_key(),
        &keys.public_key(),
        &json,
        nip44::Version::default(),
    )
    .map_err(|e| RemoteConfigError::EncryptionError(e.to_string()))?;

    let tags = vec![Tag::identifier(CONFIG_D_TAG)];
    let event = EventBuilder::new(KIND_APP_SPECIFIC_DATA, encrypted, tags)
        .to_event(keys)
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?;

    let event_id = event.id;
    client
        .send_event(event)
        .await
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?;

    tracing::info!("Saved config to relays: {}", event_id);

    Ok(event_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = RemoteConfig {
            version: 1,
            admin: Some("npub1test".to_string()),
            relays: vec!["wss://relay.damus.io".to_string()],
            blossom_servers: vec!["https://blossom.example.com".to_string()],
            blob_expiration_days: 30,
            name: Some("Test DVM".to_string()),
            about: Some("A test DVM".to_string()),
            paused: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: RemoteConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.admin, Some("npub1test".to_string()));
        assert_eq!(parsed.relays.len(), 1);
        assert!(!parsed.paused);
    }

    #[test]
    fn test_config_defaults() {
        let json = r#"{"version": 1}"#;
        let config: RemoteConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.blob_expiration_days, 30);
        assert!(config.relays.is_empty());
        assert!(!config.paused);
    }

    #[test]
    fn test_has_admin() {
        let mut config = RemoteConfig::new();
        assert!(!config.has_admin());

        config.admin = Some("npub1test".to_string());
        assert!(config.has_admin());
    }
}
