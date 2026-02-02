# Remote Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable zero-config DVM startup with full remote configuration via Nostr, allowing operators to manage DVMs through encrypted DMs.

**Architecture:** The DVM stores only its identity key locally. On startup, it connects to bootstrap relays, fetches NIP-78 encrypted config, and enters pairing mode if no admin is configured. Admin commands arrive as NIP-44 encrypted DMs and trigger hot-reload of settings.

**Tech Stack:** nostr-sdk 0.35 (NIP-44/NIP-78), tokio async, qrcode crate for terminal QR, dirs crate for platform paths

---

## Phase 1: Identity Management

### Task 1.1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add new dependencies**

Add to `[dependencies]` section in `Cargo.toml`:

```toml
qrcode = "0.14"
dirs = "5.0"
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "deps: add qrcode and dirs crates for remote config"
```

---

### Task 1.2: Create Identity Module

**Files:**
- Create: `src/identity.rs`
- Modify: `src/lib.rs`

**Step 1: Write failing test**

Create `src/identity.rs` with test first:

```rust
//! Identity key management for the DVM.
//!
//! Handles loading and generating the DVM's identity keypair.
//! The identity is stored as a 64-character hex private key.

use nostr_sdk::Keys;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("Failed to read identity file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Invalid key format: {0}")]
    InvalidKey(String),
    #[error("Failed to create data directory: {0}")]
    DirectoryError(String),
}

/// Returns the default data directory for the DVM.
///
/// - Linux/macOS: `~/.local/share/dvm-video/`
/// - Respects `DATA_DIR` environment variable if set
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DATA_DIR") {
        return PathBuf::from(dir);
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dvm-video")
}

/// Returns the path to the identity key file.
pub fn identity_key_path() -> PathBuf {
    default_data_dir().join("identity.key")
}

/// Loads or generates the DVM identity keypair.
///
/// If the identity file exists, loads the key from it.
/// Otherwise, generates a new keypair and saves it.
pub fn load_or_generate_identity() -> Result<Keys, IdentityError> {
    let key_path = identity_key_path();

    if key_path.exists() {
        load_identity(&key_path)
    } else {
        generate_and_save_identity(&key_path)
    }
}

fn load_identity(path: &PathBuf) -> Result<Keys, IdentityError> {
    let hex_key = std::fs::read_to_string(path)?
        .trim()
        .to_string();

    Keys::parse(&hex_key)
        .map_err(|e| IdentityError::InvalidKey(e.to_string()))
}

fn generate_and_save_identity(path: &PathBuf) -> Result<Keys, IdentityError> {
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| IdentityError::DirectoryError(e.to_string()))?;
    }

    let keys = Keys::generate();
    let hex_key = keys.secret_key().to_secret_hex();

    std::fs::write(path, &hex_key)?;

    // Set file permissions to 600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }

    tracing::info!("Generated new identity: {}", keys.public_key().to_bech32().unwrap_or_default());

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_new_identity() {
        let dir = tempdir().unwrap();
        std::env::set_var("DATA_DIR", dir.path().to_str().unwrap());

        let keys = load_or_generate_identity().unwrap();

        // Verify key file was created
        let key_path = dir.path().join("dvm-video").join("identity.key");
        assert!(key_path.exists());

        // Verify content is valid hex
        let content = std::fs::read_to_string(&key_path).unwrap();
        assert_eq!(content.len(), 64);
        assert!(content.chars().all(|c| c.is_ascii_hexdigit()));

        std::env::remove_var("DATA_DIR");
    }

    #[test]
    fn test_load_existing_identity() {
        let dir = tempdir().unwrap();
        std::env::set_var("DATA_DIR", dir.path().to_str().unwrap());

        // Generate first
        let keys1 = load_or_generate_identity().unwrap();

        // Load again - should get same key
        let keys2 = load_or_generate_identity().unwrap();

        assert_eq!(keys1.public_key(), keys2.public_key());

        std::env::remove_var("DATA_DIR");
    }

    #[test]
    fn test_invalid_key_format() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("dvm-video");
        std::fs::create_dir_all(&key_path).unwrap();
        std::fs::write(key_path.join("identity.key"), "invalid-key").unwrap();

        std::env::set_var("DATA_DIR", dir.path().to_str().unwrap());

        let result = load_or_generate_identity();
        assert!(result.is_err());

        std::env::remove_var("DATA_DIR");
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod identity;
```

**Step 3: Run tests**

Run: `cargo test identity --lib`
Expected: All 3 tests pass

**Step 4: Commit**

```bash
git add src/identity.rs src/lib.rs
git commit -m "feat: add identity module for keypair persistence"
```

---

## Phase 2: Remote Configuration Storage

### Task 2.1: Create Remote Config Module

**Files:**
- Create: `src/remote_config.rs`
- Modify: `src/lib.rs`

**Step 1: Define config structs and serialization**

Create `src/remote_config.rs`:

```rust
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
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?;

    let event = match events.into_iter().next() {
        Some(e) => e,
        None => return Ok(None),
    };

    // Decrypt content using NIP-44 (encrypted to self)
    let decrypted = nip44::decrypt(
        keys.secret_key(),
        &keys.public_key(),
        &event.content,
    )
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

    let event = EventBuilder::new(KIND_APP_SPECIFIC_DATA, encrypted)
        .tag(Tag::identifier(CONFIG_D_TAG))
        .sign_with_keys(keys)
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?;

    let event_id = client
        .send_event(event)
        .await
        .map_err(|e| RemoteConfigError::RelayError(e.to_string()))?
        .id();

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
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod remote_config;
```

**Step 3: Run tests**

Run: `cargo test remote_config --lib`
Expected: All 3 tests pass

**Step 4: Commit**

```bash
git add src/remote_config.rs src/lib.rs
git commit -m "feat: add remote config module with NIP-78 storage"
```

---

## Phase 3: Bootstrap Relays

### Task 3.1: Create Bootstrap Module

**Files:**
- Create: `src/bootstrap.rs`
- Modify: `src/lib.rs`

**Step 1: Implement bootstrap relay logic**

Create `src/bootstrap.rs`:

```rust
//! Bootstrap relay management.
//!
//! Provides initial relays for DVM startup before config is loaded.

use nostr_sdk::Url;

/// Default bootstrap relays used when no configuration is available
pub const DEFAULT_BOOTSTRAP_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
];

/// Default admin app URL for pairing links
pub const DEFAULT_ADMIN_APP_URL: &str = "https://dvm-admin.example.com";

/// Returns bootstrap relays from environment or defaults.
///
/// Checks `BOOTSTRAP_RELAYS` environment variable first (comma-separated).
/// Falls back to hardcoded defaults.
pub fn get_bootstrap_relays() -> Vec<Url> {
    if let Ok(relays_str) = std::env::var("BOOTSTRAP_RELAYS") {
        relays_str
            .split(',')
            .filter_map(|s| Url::parse(s.trim()).ok())
            .collect()
    } else {
        DEFAULT_BOOTSTRAP_RELAYS
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect()
    }
}

/// Returns the admin app URL for pairing links.
///
/// Checks `DVM_ADMIN_APP_URL` environment variable first.
/// Falls back to default.
pub fn get_admin_app_url() -> String {
    std::env::var("DVM_ADMIN_APP_URL")
        .unwrap_or_else(|_| DEFAULT_ADMIN_APP_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bootstrap_relays() {
        std::env::remove_var("BOOTSTRAP_RELAYS");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 3);
        assert!(relays[0].to_string().contains("damus.io"));
    }

    #[test]
    fn test_custom_bootstrap_relays() {
        std::env::set_var("BOOTSTRAP_RELAYS", "wss://custom1.com,wss://custom2.com");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 2);
        assert!(relays[0].to_string().contains("custom1.com"));

        std::env::remove_var("BOOTSTRAP_RELAYS");
    }

    #[test]
    fn test_admin_app_url_default() {
        std::env::remove_var("DVM_ADMIN_APP_URL");

        let url = get_admin_app_url();

        assert_eq!(url, DEFAULT_ADMIN_APP_URL);
    }

    #[test]
    fn test_admin_app_url_custom() {
        std::env::set_var("DVM_ADMIN_APP_URL", "https://my-admin.com");

        let url = get_admin_app_url();

        assert_eq!(url, "https://my-admin.com");

        std::env::remove_var("DVM_ADMIN_APP_URL");
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod bootstrap;
```

**Step 3: Run tests**

Run: `cargo test bootstrap --lib`
Expected: All 4 tests pass

**Step 4: Commit**

```bash
git add src/bootstrap.rs src/lib.rs
git commit -m "feat: add bootstrap relay module"
```

---

## Phase 4: Pairing Mode

### Task 4.1: Create Pairing Module

**Files:**
- Create: `src/pairing.rs`
- Modify: `src/lib.rs`

**Step 1: Implement pairing secret and display**

Create `src/pairing.rs`:

```rust
//! DVM pairing mode for admin setup.
//!
//! Generates one-time pairing secrets and displays QR codes for easy setup.

use nostr_sdk::prelude::*;
use qrcode::QrCode;
use qrcode::render::unicode;
use rand::Rng;
use std::time::{Duration, Instant};

/// Pairing secret format: xxxx-xxxx-xxxx (12 alphanumeric chars)
const SECRET_LENGTH: usize = 12;
const SECRET_CHARSET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz"; // No 0,1,i,l,o for clarity

/// Pairing secret validity duration
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(5 * 60); // 5 minutes

/// Pairing state for the DVM
#[derive(Debug)]
pub struct PairingState {
    pub secret: String,
    pub created_at: Instant,
    pub dvm_pubkey: PublicKey,
}

impl PairingState {
    /// Create new pairing state with a fresh secret
    pub fn new(dvm_pubkey: PublicKey) -> Self {
        Self {
            secret: generate_pairing_secret(),
            created_at: Instant::now(),
            dvm_pubkey,
        }
    }

    /// Check if the pairing secret is still valid
    pub fn is_valid(&self) -> bool {
        self.created_at.elapsed() < PAIRING_TIMEOUT
    }

    /// Verify a provided secret matches
    pub fn verify(&self, provided: &str) -> bool {
        self.is_valid() && constant_time_eq(self.secret.as_bytes(), provided.as_bytes())
    }

    /// Generate the pairing URL
    pub fn pairing_url(&self, base_url: &str) -> String {
        let npub = self.dvm_pubkey.to_bech32().unwrap_or_default();
        format!("{}/pair?dvm={}&secret={}", base_url, npub, self.secret)
    }

    /// Display pairing information to console
    pub fn display(&self, base_url: &str) {
        let url = self.pairing_url(base_url);
        let npub = self.dvm_pubkey.to_bech32().unwrap_or_default();

        println!("\n═══════════════════════════════════════════════════════════════");
        println!("VIDEO TRANSFORM DVM - PAIRING MODE");
        println!();
        println!("DVM pubkey: {}", npub);
        println!();
        println!("Pair this DVM by opening:");
        println!("{}", url);
        println!();

        // Generate QR code
        if let Ok(qr) = QrCode::new(&url) {
            let qr_string = qr.render::<unicode::Dense1x2>()
                .dark_color(unicode::Dense1x2::Light)
                .light_color(unicode::Dense1x2::Dark)
                .build();
            println!("Or scan:");
            println!("{}", qr_string);
        }

        println!();
        println!("Waiting for pairing request...");
        println!("═══════════════════════════════════════════════════════════════\n");
    }
}

/// Generate a random pairing secret in format xxxx-xxxx-xxxx
fn generate_pairing_secret() -> String {
    let mut rng = rand::thread_rng();
    let chars: String = (0..SECRET_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..SECRET_CHARSET.len());
            SECRET_CHARSET[idx] as char
        })
        .collect();

    // Format as xxxx-xxxx-xxxx
    format!("{}-{}-{}", &chars[0..4], &chars[4..8], &chars[8..12])
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pairing_secret() {
        let secret = generate_pairing_secret();

        // Format: xxxx-xxxx-xxxx (14 chars total with dashes)
        assert_eq!(secret.len(), 14);
        assert_eq!(secret.chars().filter(|&c| c == '-').count(), 2);

        // All chars should be from charset or dash
        for c in secret.chars() {
            if c != '-' {
                assert!(SECRET_CHARSET.contains(&(c as u8)));
            }
        }
    }

    #[test]
    fn test_pairing_secrets_are_unique() {
        let secret1 = generate_pairing_secret();
        let secret2 = generate_pairing_secret();

        assert_ne!(secret1, secret2);
    }

    #[test]
    fn test_pairing_state_validity() {
        let keys = Keys::generate();
        let state = PairingState::new(keys.public_key());

        assert!(state.is_valid());
        assert!(state.verify(&state.secret));
        assert!(!state.verify("wrong-secret"));
    }

    #[test]
    fn test_pairing_url() {
        let keys = Keys::generate();
        let state = PairingState::new(keys.public_key());

        let url = state.pairing_url("https://example.com");

        assert!(url.starts_with("https://example.com/pair?"));
        assert!(url.contains("dvm=npub"));
        assert!(url.contains(&format!("secret={}", state.secret)));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"test", b"test"));
        assert!(!constant_time_eq(b"test", b"TEST"));
        assert!(!constant_time_eq(b"test", b"test1"));
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod pairing;
```

**Step 3: Run tests**

Run: `cargo test pairing --lib`
Expected: All 5 tests pass

**Step 4: Commit**

```bash
git add src/pairing.rs src/lib.rs
git commit -m "feat: add pairing module with QR code support"
```

---

## Phase 5: Admin Commands

### Task 5.1: Create Admin Commands Module

**Files:**
- Create: `src/admin/mod.rs`
- Create: `src/admin/commands.rs`
- Modify: `src/lib.rs`

**Step 1: Define command types**

Create `src/admin/mod.rs`:

```rust
//! Admin command handling via encrypted DMs.

pub mod commands;

pub use commands::*;
```

Create `src/admin/commands.rs`:

```rust
//! Admin command parsing and response types.
//!
//! Commands are sent as NIP-44 encrypted DMs from the admin to the DVM.

use serde::{Deserialize, Serialize};

/// Admin command request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AdminCommand {
    /// Claim admin role during pairing
    ClaimAdmin { secret: String },

    /// Get current configuration
    GetConfig,

    /// Set relay list
    SetRelays { relays: Vec<String> },

    /// Set Blossom servers
    SetBlossomServers { servers: Vec<String> },

    /// Set blob expiration
    SetBlobExpiration { days: u32 },

    /// Set DVM profile
    SetProfile {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        about: Option<String>,
    },

    /// Pause DVM (reject new jobs)
    Pause,

    /// Resume DVM
    Resume,

    /// Get DVM status
    Status,

    /// Get job history
    JobHistory {
        #[serde(default = "default_limit")]
        limit: u32,
    },

    /// Run self-test
    SelfTest,

    /// Import configuration from environment
    ImportEnvConfig,
}

fn default_limit() -> u32 {
    20
}

/// Admin command response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(flatten)]
    pub data: Option<ResponseData>,
}

/// Response data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    Config(ConfigResponse),
    Status(StatusResponse),
    JobHistory(JobHistoryResponse),
    SelfTest(SelfTestResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub config: ConfigData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigData {
    pub relays: Vec<String>,
    pub blossom_servers: Vec<String>,
    pub blob_expiration_days: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub paused: bool,
    pub jobs_active: u32,
    pub jobs_completed: u32,
    pub jobs_failed: u32,
    pub uptime_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwaccel: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobHistoryResponse {
    pub jobs: Vec<JobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: String,
    pub status: String,
    pub input_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_url: Option<String>,
    pub started_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfTestResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_duration_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_time_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwaccel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AdminResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            msg: None,
            data: None,
        }
    }

    pub fn ok_with_msg(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            error: None,
            msg: Some(msg.into()),
            data: None,
        }
    }

    pub fn ok_with_data(data: ResponseData) -> Self {
        Self {
            ok: true,
            error: None,
            msg: None,
            data: Some(data),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            msg: None,
            data: None,
        }
    }
}

/// Parse an admin command from JSON
pub fn parse_command(json: &str) -> Result<AdminCommand, serde_json::Error> {
    serde_json::from_str(json)
}

/// Serialize an admin response to JSON
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

        match cmd {
            AdminCommand::ClaimAdmin { secret } => {
                assert_eq!(secret, "abc1-def2-ghi3");
            }
            _ => panic!("Expected ClaimAdmin"),
        }
    }

    #[test]
    fn test_parse_get_config() {
        let json = r#"{"cmd": "get_config"}"#;
        let cmd = parse_command(json).unwrap();

        assert!(matches!(cmd, AdminCommand::GetConfig));
    }

    #[test]
    fn test_parse_set_relays() {
        let json = r#"{"cmd": "set_relays", "relays": ["wss://relay1.com", "wss://relay2.com"]}"#;
        let cmd = parse_command(json).unwrap();

        match cmd {
            AdminCommand::SetRelays { relays } => {
                assert_eq!(relays.len(), 2);
            }
            _ => panic!("Expected SetRelays"),
        }
    }

    #[test]
    fn test_parse_set_profile() {
        let json = r#"{"cmd": "set_profile", "name": "My DVM", "about": "Description"}"#;
        let cmd = parse_command(json).unwrap();

        match cmd {
            AdminCommand::SetProfile { name, about } => {
                assert_eq!(name, Some("My DVM".to_string()));
                assert_eq!(about, Some("Description".to_string()));
            }
            _ => panic!("Expected SetProfile"),
        }
    }

    #[test]
    fn test_parse_job_history_default_limit() {
        let json = r#"{"cmd": "job_history"}"#;
        let cmd = parse_command(json).unwrap();

        match cmd {
            AdminCommand::JobHistory { limit } => {
                assert_eq!(limit, 20);
            }
            _ => panic!("Expected JobHistory"),
        }
    }

    #[test]
    fn test_serialize_ok_response() {
        let response = AdminResponse::ok();
        let json = serialize_response(&response).unwrap();

        assert!(json.contains(r#""ok":true"#));
    }

    #[test]
    fn test_serialize_error_response() {
        let response = AdminResponse::error("Something went wrong");
        let json = serialize_response(&response).unwrap();

        assert!(json.contains(r#""ok":false"#));
        assert!(json.contains("Something went wrong"));
    }

    #[test]
    fn test_serialize_config_response() {
        let config = ConfigData {
            relays: vec!["wss://relay.test".to_string()],
            blossom_servers: vec![],
            blob_expiration_days: 30,
            name: Some("Test".to_string()),
            about: None,
            paused: false,
        };
        let response = AdminResponse::ok_with_data(
            ResponseData::Config(ConfigResponse { config })
        );
        let json = serialize_response(&response).unwrap();

        assert!(json.contains("relay.test"));
        assert!(json.contains(r#""paused":false"#));
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod admin;
```

**Step 3: Run tests**

Run: `cargo test admin --lib`
Expected: All 8 tests pass

**Step 4: Commit**

```bash
git add src/admin src/lib.rs
git commit -m "feat: add admin command types and parsing"
```

---

## Phase 6: DVM State Management

### Task 6.1: Create DVM State Module

**Files:**
- Create: `src/dvm_state.rs`
- Modify: `src/lib.rs`

**Step 1: Implement shared DVM state**

Create `src/dvm_state.rs`:

```rust
//! Shared DVM runtime state.
//!
//! Manages configuration, job tracking, and operational state that can be
//! hot-reloaded without restart.

use crate::remote_config::RemoteConfig;
use nostr_sdk::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Maximum number of jobs to keep in history
const MAX_JOB_HISTORY: usize = 100;

/// Shared DVM state wrapped for concurrent access
pub type SharedDvmState = Arc<RwLock<DvmState>>;

/// Runtime state of the DVM
#[derive(Debug)]
pub struct DvmState {
    /// Remote configuration
    pub config: RemoteConfig,
    /// DVM identity keys
    pub keys: Keys,
    /// Start time for uptime calculation
    pub started_at: Instant,
    /// Number of currently active jobs
    pub jobs_active: u32,
    /// Total completed jobs
    pub jobs_completed: u32,
    /// Total failed jobs
    pub jobs_failed: u32,
    /// Recent job history (ring buffer)
    pub job_history: VecDeque<JobRecord>,
    /// Detected hardware acceleration
    pub hwaccel: Option<String>,
}

/// Record of a processed job
#[derive(Debug, Clone)]
pub struct JobRecord {
    pub id: String,
    pub status: JobStatus,
    pub input_url: String,
    pub output_url: Option<String>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobStatus {
    Processing,
    Completed,
    Failed,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Processing => write!(f, "processing"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
        }
    }
}

impl DvmState {
    /// Create new DVM state with the given keys and config
    pub fn new(keys: Keys, config: RemoteConfig) -> Self {
        Self {
            config,
            keys,
            started_at: Instant::now(),
            jobs_active: 0,
            jobs_completed: 0,
            jobs_failed: 0,
            job_history: VecDeque::with_capacity(MAX_JOB_HISTORY),
            hwaccel: None,
        }
    }

    /// Create new shared state
    pub fn new_shared(keys: Keys, config: RemoteConfig) -> SharedDvmState {
        Arc::new(RwLock::new(Self::new(keys, config)))
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Check if DVM is paused
    pub fn is_paused(&self) -> bool {
        self.config.paused
    }

    /// Record job start
    pub fn job_started(&mut self, id: String, input_url: String) {
        self.jobs_active += 1;

        let record = JobRecord {
            id,
            status: JobStatus::Processing,
            input_url,
            output_url: None,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            completed_at: None,
        };

        // Add to history, remove oldest if full
        if self.job_history.len() >= MAX_JOB_HISTORY {
            self.job_history.pop_front();
        }
        self.job_history.push_back(record);
    }

    /// Record job completion
    pub fn job_completed(&mut self, id: &str, output_url: Option<String>) {
        self.jobs_active = self.jobs_active.saturating_sub(1);
        self.jobs_completed += 1;

        if let Some(record) = self.job_history.iter_mut().rev().find(|r| r.id == id) {
            record.status = JobStatus::Completed;
            record.output_url = output_url;
            record.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
        }
    }

    /// Record job failure
    pub fn job_failed(&mut self, id: &str) {
        self.jobs_active = self.jobs_active.saturating_sub(1);
        self.jobs_failed += 1;

        if let Some(record) = self.job_history.iter_mut().rev().find(|r| r.id == id) {
            record.status = JobStatus::Failed;
            record.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
        }
    }

    /// Get recent job history
    pub fn get_job_history(&self, limit: usize) -> Vec<&JobRecord> {
        self.job_history.iter().rev().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let keys = Keys::generate();
        let config = RemoteConfig::new();
        let state = DvmState::new(keys.clone(), config);

        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_completed, 0);
        assert!(!state.is_paused());
    }

    #[test]
    fn test_job_lifecycle() {
        let keys = Keys::generate();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        // Start job
        state.job_started("job1".to_string(), "https://example.com/video.mp4".to_string());
        assert_eq!(state.jobs_active, 1);
        assert_eq!(state.job_history.len(), 1);

        // Complete job
        state.job_completed("job1", Some("https://blossom.com/result.m3u8".to_string()));
        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_completed, 1);

        let record = &state.job_history[0];
        assert_eq!(record.status, JobStatus::Completed);
        assert!(record.output_url.is_some());
    }

    #[test]
    fn test_job_failure() {
        let keys = Keys::generate();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        state.job_started("job1".to_string(), "https://example.com/video.mp4".to_string());
        state.job_failed("job1");

        assert_eq!(state.jobs_active, 0);
        assert_eq!(state.jobs_failed, 1);
        assert_eq!(state.job_history[0].status, JobStatus::Failed);
    }

    #[test]
    fn test_job_history_limit() {
        let keys = Keys::generate();
        let config = RemoteConfig::new();
        let mut state = DvmState::new(keys, config);

        // Add more than MAX_JOB_HISTORY jobs
        for i in 0..150 {
            state.job_started(format!("job{}", i), "https://example.com/video.mp4".to_string());
            state.job_completed(&format!("job{}", i), None);
        }

        assert!(state.job_history.len() <= MAX_JOB_HISTORY);
    }

    #[test]
    fn test_paused_state() {
        let keys = Keys::generate();
        let mut config = RemoteConfig::new();
        config.paused = true;
        let state = DvmState::new(keys, config);

        assert!(state.is_paused());
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod dvm_state;
```

**Step 3: Run tests**

Run: `cargo test dvm_state --lib`
Expected: All 5 tests pass

**Step 4: Commit**

```bash
git add src/dvm_state.rs src/lib.rs
git commit -m "feat: add DVM state management with job tracking"
```

---

## Phase 7: Admin Command Handler

### Task 7.1: Create Admin Handler

**Files:**
- Create: `src/admin/handler.rs`
- Modify: `src/admin/mod.rs`

**Step 1: Implement command handler**

Create `src/admin/handler.rs`:

```rust
//! Admin command handler.
//!
//! Processes admin commands received via encrypted DMs and returns responses.

use crate::admin::commands::*;
use crate::dvm_state::{DvmState, SharedDvmState};
use crate::pairing::PairingState;
use crate::remote_config::{save_config, RemoteConfig};
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handles admin command processing
pub struct AdminHandler {
    state: SharedDvmState,
    client: Client,
    pairing: Arc<RwLock<Option<PairingState>>>,
}

impl AdminHandler {
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

    /// Process an admin command and return the response
    pub async fn handle(&self, command: AdminCommand, sender: PublicKey) -> AdminResponse {
        // Check authorization for non-pairing commands
        if !matches!(command, AdminCommand::ClaimAdmin { .. }) {
            let state = self.state.read().await;
            match state.config.admin_pubkey() {
                Some(admin) if admin == sender => {}
                Some(_) => return AdminResponse::error("Unauthorized: not admin"),
                None => return AdminResponse::error("DVM not configured - use claim_admin first"),
            }
        }

        match command {
            AdminCommand::ClaimAdmin { secret } => self.handle_claim_admin(secret, sender).await,
            AdminCommand::GetConfig => self.handle_get_config().await,
            AdminCommand::SetRelays { relays } => self.handle_set_relays(relays).await,
            AdminCommand::SetBlossomServers { servers } => self.handle_set_blossom(servers).await,
            AdminCommand::SetBlobExpiration { days } => self.handle_set_expiration(days).await,
            AdminCommand::SetProfile { name, about } => self.handle_set_profile(name, about).await,
            AdminCommand::Pause => self.handle_pause().await,
            AdminCommand::Resume => self.handle_resume().await,
            AdminCommand::Status => self.handle_status().await,
            AdminCommand::JobHistory { limit } => self.handle_job_history(limit).await,
            AdminCommand::SelfTest => self.handle_self_test().await,
            AdminCommand::ImportEnvConfig => self.handle_import_env().await,
        }
    }

    async fn handle_claim_admin(&self, secret: String, sender: PublicKey) -> AdminResponse {
        // Check if already has admin
        {
            let state = self.state.read().await;
            if state.config.has_admin() {
                return AdminResponse::error("DVM already has an admin configured");
            }
        }

        // Verify pairing secret
        {
            let pairing = self.pairing.read().await;
            match pairing.as_ref() {
                Some(p) if p.verify(&secret) => {}
                Some(_) => return AdminResponse::error("Invalid or expired pairing secret"),
                None => return AdminResponse::error("DVM not in pairing mode"),
            }
        }

        // Set admin and save config
        {
            let mut state = self.state.write().await;
            state.config.admin = Some(sender.to_hex());

            if let Err(e) = save_config(&self.client, &state.keys, &state.config).await {
                tracing::error!("Failed to save config: {}", e);
                return AdminResponse::error(format!("Failed to save config: {}", e));
            }
        }

        // Clear pairing state
        {
            let mut pairing = self.pairing.write().await;
            *pairing = None;
        }

        tracing::info!("Admin claimed: {}", sender.to_bech32().unwrap_or_default());
        AdminResponse::ok_with_msg("Admin claimed successfully")
    }

    async fn handle_get_config(&self) -> AdminResponse {
        let state = self.state.read().await;
        let config = ConfigData {
            relays: state.config.relays.clone(),
            blossom_servers: state.config.blossom_servers.clone(),
            blob_expiration_days: state.config.blob_expiration_days,
            name: state.config.name.clone(),
            about: state.config.about.clone(),
            paused: state.config.paused,
        };
        AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse { config }))
    }

    async fn handle_set_relays(&self, relays: Vec<String>) -> AdminResponse {
        // Validate relay URLs
        for relay in &relays {
            if Url::parse(relay).is_err() {
                return AdminResponse::error(format!("Invalid relay URL: {}", relay));
            }
        }

        let keys = {
            let mut state = self.state.write().await;
            state.config.relays = relays.clone();
            state.keys.clone()
        };

        // Save config
        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        // TODO: Trigger relay reconnection
        tracing::info!("Relays updated: {:?}", relays);
        AdminResponse::ok()
    }

    async fn handle_set_blossom(&self, servers: Vec<String>) -> AdminResponse {
        // Validate server URLs
        for server in &servers {
            if Url::parse(server).is_err() {
                return AdminResponse::error(format!("Invalid server URL: {}", server));
            }
        }

        let keys = {
            let mut state = self.state.write().await;
            state.config.blossom_servers = servers.clone();
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        tracing::info!("Blossom servers updated: {:?}", servers);
        AdminResponse::ok()
    }

    async fn handle_set_expiration(&self, days: u32) -> AdminResponse {
        if days == 0 {
            return AdminResponse::error("Expiration must be at least 1 day");
        }

        let keys = {
            let mut state = self.state.write().await;
            state.config.blob_expiration_days = days;
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        tracing::info!("Blob expiration updated: {} days", days);
        AdminResponse::ok()
    }

    async fn handle_set_profile(&self, name: Option<String>, about: Option<String>) -> AdminResponse {
        let keys = {
            let mut state = self.state.write().await;
            if name.is_some() {
                state.config.name = name.clone();
            }
            if about.is_some() {
                state.config.about = about.clone();
            }
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        // TODO: Update kind 0 profile and kind 31990 announcement
        tracing::info!("Profile updated: name={:?}, about={:?}", name, about);
        AdminResponse::ok()
    }

    async fn handle_pause(&self) -> AdminResponse {
        let keys = {
            let mut state = self.state.write().await;
            state.config.paused = true;
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        tracing::info!("DVM paused");
        AdminResponse::ok_with_msg("DVM paused, rejecting new jobs")
    }

    async fn handle_resume(&self) -> AdminResponse {
        let keys = {
            let mut state = self.state.write().await;
            state.config.paused = false;
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        tracing::info!("DVM resumed");
        AdminResponse::ok_with_msg("DVM resumed")
    }

    async fn handle_status(&self) -> AdminResponse {
        let state = self.state.read().await;
        let status = StatusResponse {
            paused: state.config.paused,
            jobs_active: state.jobs_active,
            jobs_completed: state.jobs_completed,
            jobs_failed: state.jobs_failed,
            uptime_secs: state.uptime_secs(),
            hwaccel: state.hwaccel.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        AdminResponse::ok_with_data(ResponseData::Status(status))
    }

    async fn handle_job_history(&self, limit: u32) -> AdminResponse {
        let state = self.state.read().await;
        let jobs: Vec<JobInfo> = state
            .get_job_history(limit as usize)
            .iter()
            .map(|r| JobInfo {
                id: r.id.clone(),
                status: r.status.to_string(),
                input_url: r.input_url.clone(),
                output_url: r.output_url.clone(),
                started_at: r.started_at,
                completed_at: r.completed_at,
                duration_secs: r.completed_at.map(|c| c.saturating_sub(r.started_at)),
            })
            .collect();
        AdminResponse::ok_with_data(ResponseData::JobHistory(JobHistoryResponse { jobs }))
    }

    async fn handle_self_test(&self) -> AdminResponse {
        // TODO: Run actual self-test using VideoProcessor
        AdminResponse::ok_with_data(ResponseData::SelfTest(SelfTestResponse {
            success: true,
            video_duration_secs: None,
            encode_time_secs: None,
            speed_ratio: None,
            hwaccel: None,
            resolution: None,
            error: Some("Self-test not yet implemented".to_string()),
        }))
    }

    async fn handle_import_env(&self) -> AdminResponse {
        // Import from environment variables
        let relays: Vec<String> = std::env::var("NOSTR_RELAYS")
            .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
            .unwrap_or_default();

        let blossom: Vec<String> = std::env::var("BLOSSOM_UPLOAD_SERVERS")
            .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
            .unwrap_or_default();

        let expiration: u32 = std::env::var("BLOSSOM_BLOB_EXPIRATION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let name = std::env::var("DVM_NAME").ok();
        let about = std::env::var("DVM_ABOUT").ok();

        let keys = {
            let mut state = self.state.write().await;
            if !relays.is_empty() {
                state.config.relays = relays;
            }
            if !blossom.is_empty() {
                state.config.blossom_servers = blossom;
            }
            state.config.blob_expiration_days = expiration;
            state.config.name = name;
            state.config.about = about;
            state.keys.clone()
        };

        let state = self.state.read().await;
        if let Err(e) = save_config(&self.client, &keys, &state.config).await {
            return AdminResponse::error(format!("Failed to save config: {}", e));
        }

        tracing::info!("Config imported from environment");
        AdminResponse::ok_with_msg("Config imported from environment variables")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::PairingState;

    // Note: Full integration tests require a mock Nostr client
    // Unit tests focus on authorization logic

    #[tokio::test]
    async fn test_unauthorized_command() {
        let keys = Keys::generate();
        let mut config = RemoteConfig::new();
        config.admin = Some(Keys::generate().public_key().to_hex()); // Different admin

        let state = DvmState::new_shared(keys.clone(), config);
        let client = Client::new(keys.clone());
        let pairing = Arc::new(RwLock::new(None));

        let handler = AdminHandler::new(state, client, pairing);
        let unauthorized_user = Keys::generate().public_key();

        let response = handler.handle(AdminCommand::GetConfig, unauthorized_user).await;

        assert!(!response.ok);
        assert!(response.error.unwrap().contains("Unauthorized"));
    }
}
```

**Step 2: Update admin/mod.rs**

Update `src/admin/mod.rs`:

```rust
//! Admin command handling via encrypted DMs.

pub mod commands;
pub mod handler;

pub use commands::*;
pub use handler::AdminHandler;
```

**Step 3: Run tests**

Run: `cargo test admin --lib`
Expected: All tests pass (9 total)

**Step 4: Commit**

```bash
git add src/admin/handler.rs src/admin/mod.rs
git commit -m "feat: add admin command handler"
```

---

## Phase 8: Integration

### Task 8.1: Update Config Module for Hybrid Mode

**Files:**
- Modify: `src/config.rs`

**Step 1: Add hybrid config loading**

Add to `src/config.rs` (add after existing impl block):

```rust
use crate::remote_config::RemoteConfig;

impl Config {
    /// Create Config from RemoteConfig (for remote-configured mode)
    pub fn from_remote(
        keys: Keys,
        remote: &RemoteConfig,
        ffmpeg_path: PathBuf,
        ffprobe_path: PathBuf,
    ) -> Result<Self, ConfigError> {
        let relays: Vec<Url> = remote
            .relays
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();

        let blossom: Vec<Url> = remote
            .blossom_servers
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();

        let temp_dir = std::env::var("TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./temp"));

        let http_port = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3000);

        Ok(Self {
            nostr_keys: keys,
            nostr_relays: relays,
            blossom_servers: blossom,
            blob_expiration_days: remote.blob_expiration_days,
            temp_dir,
            ffmpeg_path,
            ffprobe_path,
            http_port,
            dvm_name: remote.name.clone(),
            dvm_about: remote.about.clone(),
        })
    }
}
```

**Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: add Config::from_remote for remote config mode"
```

---

### Task 8.2: Create Startup Orchestrator

**Files:**
- Create: `src/startup.rs`
- Modify: `src/lib.rs`

**Step 1: Implement startup logic**

Create `src/startup.rs`:

```rust
//! DVM startup orchestration.
//!
//! Handles the complete startup sequence including identity loading,
//! config fetching, and pairing mode.

use crate::bootstrap::{get_admin_app_url, get_bootstrap_relays};
use crate::dvm_state::{DvmState, SharedDvmState};
use crate::identity::load_or_generate_identity;
use crate::pairing::PairingState;
use crate::remote_config::{fetch_config, RemoteConfig};
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
        pairing_state.display(&get_admin_app_url());

        *pairing.write().await = Some(pairing_state);
    } else {
        tracing::info!(
            "Admin configured: {}",
            remote_config.admin.as_deref().unwrap_or("unknown")
        );

        // Connect to configured relays (replacing bootstrap)
        if !remote_config.relays.is_empty() {
            tracing::info!("Switching to configured relays...");
            // Note: In production, we'd disconnect bootstrap and connect to config relays
            for relay in &remote_config.relays {
                if let Err(e) = client.add_relay(relay.clone()).await {
                    tracing::warn!("Failed to add relay {}: {}", relay, e);
                }
            }
        }
    }

    // Step 5: Create DVM state
    let state = DvmState::new_shared(keys.clone(), remote_config);

    Ok(StartupResult {
        keys,
        client,
        state,
        pairing,
        needs_pairing,
    })
}

/// Publish initial profile for a new DVM.
pub async fn publish_initial_profile(
    client: &Client,
    keys: &Keys,
) -> Result<(), Box<dyn std::error::Error>> {
    // Kind 0: Profile
    let metadata = Metadata::new()
        .name("Video Transform DVM")
        .about("Unconfigured DVM - awaiting operator");

    let event = EventBuilder::metadata(&metadata)
        .sign_with_keys(keys)?;

    client.send_event(event).await?;
    tracing::info!("Published initial profile (kind 0)");

    // Kind 10002: Relay list
    let relays: Vec<(Url, Option<RelayMetadata>)> = get_bootstrap_relays()
        .into_iter()
        .map(|r| (r, None))
        .collect();

    let event = EventBuilder::relay_list(relays)
        .sign_with_keys(keys)?;

    client.send_event(event).await?;
    tracing::info!("Published relay list (kind 10002)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup_result_fields() {
        // Just verify the struct can be created
        let keys = Keys::generate();
        let client = Client::new(keys.clone());
        let state = DvmState::new_shared(keys.clone(), RemoteConfig::new());
        let pairing = Arc::new(RwLock::new(None));

        let result = StartupResult {
            keys: keys.clone(),
            client,
            state,
            pairing,
            needs_pairing: true,
        };

        assert!(result.needs_pairing);
        assert_eq!(result.keys.public_key(), keys.public_key());
    }
}
```

**Step 2: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod startup;
```

**Step 3: Run tests**

Run: `cargo test startup --lib`
Expected: Test passes

**Step 4: Commit**

```bash
git add src/startup.rs src/lib.rs
git commit -m "feat: add startup orchestrator for remote config"
```

---

## Phase 9: Admin DM Listener

### Task 9.1: Create DM Subscription Handler

**Files:**
- Create: `src/admin/listener.rs`
- Modify: `src/admin/mod.rs`

**Step 1: Implement DM listener**

Create `src/admin/listener.rs`:

```rust
//! Admin DM listener.
//!
//! Subscribes to encrypted DMs from the admin and processes commands.

use crate::admin::commands::{parse_command, serialize_response};
use crate::admin::handler::AdminHandler;
use crate::dvm_state::SharedDvmState;
use crate::pairing::PairingState;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Starts listening for admin DMs and processes commands.
pub async fn run_admin_listener(
    client: Client,
    keys: Keys,
    state: SharedDvmState,
    pairing: Arc<RwLock<Option<PairingState>>>,
) {
    let handler = AdminHandler::new(state.clone(), client.clone(), pairing);

    // Subscribe to gift-wrapped DMs (NIP-44/NIP-17) and legacy NIP-04
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(keys.public_key())
        .since(Timestamp::now());

    let filter_legacy = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(keys.public_key())
        .since(Timestamp::now());

    if let Err(e) = client.subscribe(vec![filter, filter_legacy], None).await {
        tracing::error!("Failed to subscribe to DMs: {}", e);
        return;
    }

    tracing::info!("Listening for admin DMs...");

    // Handle incoming events
    client
        .handle_notifications(|notification| async {
            if let RelayPoolNotification::Event { event, .. } = notification {
                match event.kind {
                    Kind::GiftWrap => {
                        handle_gift_wrap(&event, &keys, &handler, &client).await;
                    }
                    Kind::EncryptedDirectMessage => {
                        handle_nip04_dm(&event, &keys, &handler, &client).await;
                    }
                    _ => {}
                }
            }
            Ok(false) // Continue listening
        })
        .await
        .ok();
}

async fn handle_gift_wrap(event: &Event, keys: &Keys, handler: &AdminHandler, client: &Client) {
    // Unwrap the gift wrap to get the sealed sender
    let unwrapped = match nip59::extract_rumor(keys, event) {
        Ok(rumor) => rumor,
        Err(e) => {
            tracing::debug!("Failed to unwrap gift: {}", e);
            return;
        }
    };

    let sender = unwrapped.pubkey;
    let content = &unwrapped.content;

    process_command(content, sender, keys, handler, client).await;
}

async fn handle_nip04_dm(event: &Event, keys: &Keys, handler: &AdminHandler, client: &Client) {
    // Decrypt NIP-04 message
    let content = match nip04::decrypt(keys.secret_key(), &event.pubkey, &event.content) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to decrypt NIP-04 DM: {}", e);
            return;
        }
    };

    let sender = event.pubkey;
    process_command(&content, sender, keys, handler, client).await;
}

async fn process_command(
    content: &str,
    sender: PublicKey,
    keys: &Keys,
    handler: &AdminHandler,
    client: &Client,
) {
    // Parse command
    let command = match parse_command(content) {
        Ok(cmd) => {
            tracing::info!(
                "Received admin command from {}: {:?}",
                sender.to_bech32().unwrap_or_default(),
                cmd
            );
            cmd
        }
        Err(e) => {
            tracing::debug!("Failed to parse command: {}", e);
            return;
        }
    };

    // Process command
    let response = handler.handle(command, sender).await;

    // Send response as encrypted DM
    let response_json = match serialize_response(&response) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize response: {}", e);
            return;
        }
    };

    // Use NIP-17 (gift wrap) for response
    if let Err(e) = send_dm_response(client, keys, &sender, &response_json).await {
        tracing::error!("Failed to send response: {}", e);
    }
}

async fn send_dm_response(
    client: &Client,
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create a gift-wrapped DM (NIP-17)
    let event = EventBuilder::sealed_direct(recipient.clone(), content)?
        .sign_with_keys(keys)?;

    // Send gift wrap
    let gift = EventBuilder::gift_wrap(keys, recipient, event, None)?;

    client.send_event(gift).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::commands::AdminCommand;

    #[test]
    fn test_parse_valid_command() {
        let json = r#"{"cmd": "status"}"#;
        let cmd = parse_command(json).unwrap();
        assert!(matches!(cmd, AdminCommand::Status));
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_command("not json");
        assert!(result.is_err());
    }
}
```

**Step 2: Update admin/mod.rs**

Update `src/admin/mod.rs`:

```rust
//! Admin command handling via encrypted DMs.

pub mod commands;
pub mod handler;
pub mod listener;

pub use commands::*;
pub use handler::AdminHandler;
pub use listener::run_admin_listener;
```

**Step 3: Run tests**

Run: `cargo test admin --lib`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/admin/listener.rs src/admin/mod.rs
git commit -m "feat: add admin DM listener with NIP-17 support"
```

---

## Phase 10: Main Integration

### Task 10.1: Update Main Entry Point

**Files:**
- Modify: `src/main.rs`

**Step 1: Add remote config startup path**

This task integrates all the pieces. Update `src/main.rs` to add a new startup path that uses the remote config system while maintaining backward compatibility with environment variables.

Add these imports at the top:

```rust
use dvm_video_processing::admin::run_admin_listener;
use dvm_video_processing::startup::initialize;
```

Add a feature flag check early in main():

```rust
// Check if we should use remote config mode
let use_remote_config = std::env::var("NOSTR_PRIVATE_KEY").is_err();
```

Add the remote config startup branch:

```rust
if use_remote_config {
    // Remote config mode - zero config startup
    let startup = initialize().await
        .expect("Failed to initialize DVM");

    // Spawn admin listener
    let admin_client = startup.client.clone();
    let admin_keys = startup.keys.clone();
    let admin_state = startup.state.clone();
    let admin_pairing = startup.pairing.clone();

    tokio::spawn(async move {
        run_admin_listener(admin_client, admin_keys, admin_state, admin_pairing).await;
    });

    if startup.needs_pairing {
        // In pairing mode, just wait for admin to claim
        tracing::info!("Waiting for admin pairing...");
        // The admin listener will handle the claim_admin command
        // and transition to normal operation
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let state = startup.state.read().await;
            if state.config.has_admin() {
                tracing::info!("Admin paired, starting normal operation");
                break;
            }
        }
    }

    // Continue with normal operation using remote config
    // ... (existing job processing code, adapted to use startup.state)
} else {
    // Legacy mode - use environment variables
    // ... (existing code)
}
```

**Step 2: Run tests and verify compilation**

Run: `cargo check`
Expected: Compiles without errors

Run: `cargo test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: integrate remote config startup in main"
```

---

## Phase 11: Documentation & Cleanup

### Task 11.1: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add remote config documentation**

Add section to CLAUDE.md:

```markdown
## Remote Configuration

The DVM supports two startup modes:

### Environment Variable Mode (Legacy)
Set `NOSTR_PRIVATE_KEY` and other env vars as before. The DVM uses these directly.

### Remote Config Mode (Zero-Config)
Run without `NOSTR_PRIVATE_KEY`. The DVM will:
1. Generate an identity (saved to `~/.local/share/dvm-video/identity.key`)
2. Connect to bootstrap relays
3. Enter pairing mode, displaying a QR code
4. Wait for admin to claim via the web app
5. Load configuration from Nostr (NIP-78)

### Key Modules

- `src/identity.rs` - Identity key persistence
- `src/bootstrap.rs` - Bootstrap relay management
- `src/remote_config.rs` - NIP-78 config storage
- `src/pairing.rs` - Pairing mode and QR codes
- `src/admin/` - Admin command handling
- `src/dvm_state.rs` - Runtime state management
- `src/startup.rs` - Startup orchestration

### Admin Commands

Sent as NIP-44 encrypted DMs:
- `get_config` - Get current configuration
- `set_relays` - Update relay list
- `set_blossom_servers` - Update Blossom servers
- `set_profile` - Update name/about
- `pause` / `resume` - Control job processing
- `status` - Get DVM status
- `job_history` - Get recent jobs
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add remote configuration documentation"
```

---

### Task 11.2: Add Integration Test

**Files:**
- Create: `tests/remote_config_integration.rs`

**Step 1: Write integration test**

Create `tests/remote_config_integration.rs`:

```rust
//! Integration tests for remote configuration.

use dvm_video_processing::admin::commands::{parse_command, AdminCommand, AdminResponse};
use dvm_video_processing::bootstrap::get_bootstrap_relays;
use dvm_video_processing::identity::load_or_generate_identity;
use dvm_video_processing::pairing::PairingState;
use dvm_video_processing::remote_config::RemoteConfig;
use tempfile::tempdir;

#[test]
fn test_full_pairing_flow() {
    // Setup temp directory for identity
    let dir = tempdir().unwrap();
    std::env::set_var("DATA_DIR", dir.path().to_str().unwrap());

    // Step 1: Generate identity
    let keys = load_or_generate_identity().unwrap();
    assert!(dir.path().join("dvm-video").join("identity.key").exists());

    // Step 2: Create pairing state
    let pairing = PairingState::new(keys.public_key());
    let secret = pairing.secret.clone();

    // Step 3: Verify pairing secret
    assert!(pairing.verify(&secret));
    assert!(!pairing.verify("wrong-secret"));

    // Step 4: Parse claim command
    let claim_json = format!(r#"{{"cmd": "claim_admin", "secret": "{}"}}"#, secret);
    let command = parse_command(&claim_json).unwrap();

    match command {
        AdminCommand::ClaimAdmin { secret: s } => assert_eq!(s, secret),
        _ => panic!("Expected ClaimAdmin"),
    }

    // Cleanup
    std::env::remove_var("DATA_DIR");
}

#[test]
fn test_config_roundtrip() {
    let config = RemoteConfig {
        version: 1,
        admin: Some("npub1testadmin".to_string()),
        relays: vec!["wss://relay1.com".to_string()],
        blossom_servers: vec!["https://blossom.test".to_string()],
        blob_expiration_days: 30,
        name: Some("Test DVM".to_string()),
        about: Some("A test DVM".to_string()),
        paused: false,
    };

    // Serialize
    let json = serde_json::to_string(&config).unwrap();

    // Deserialize
    let parsed: RemoteConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.admin, config.admin);
    assert_eq!(parsed.relays, config.relays);
    assert_eq!(parsed.name, config.name);
}

#[test]
fn test_bootstrap_relays() {
    std::env::remove_var("BOOTSTRAP_RELAYS");
    let relays = get_bootstrap_relays();

    assert!(!relays.is_empty());
    assert!(relays.iter().any(|r| r.to_string().contains("damus.io")));
}

#[test]
fn test_admin_response_serialization() {
    let response = AdminResponse::ok_with_msg("Test message");
    let json = serde_json::to_string(&response).unwrap();

    assert!(json.contains(r#""ok":true"#));
    assert!(json.contains("Test message"));
}
```

**Step 2: Run integration tests**

Run: `cargo test --test remote_config_integration`
Expected: All 4 tests pass

**Step 3: Commit**

```bash
git add tests/remote_config_integration.rs
git commit -m "test: add remote config integration tests"
```

---

## Summary

This plan implements the remote configuration system in 11 phases:

1. **Identity Management** - Persistent keypair storage
2. **Remote Config Storage** - NIP-78 encrypted config on Nostr
3. **Bootstrap Relays** - Initial relay connection
4. **Pairing Mode** - QR code and secret-based admin claim
5. **Admin Commands** - Command parsing and response types
6. **DVM State** - Shared runtime state with job tracking
7. **Admin Handler** - Command processing logic
8. **Config Integration** - Hybrid config loading
9. **Admin DM Listener** - NIP-17/NIP-04 DM subscription
10. **Main Integration** - Entry point updates
11. **Documentation** - CLAUDE.md and integration tests

Each task follows TDD with failing test first, then implementation.
