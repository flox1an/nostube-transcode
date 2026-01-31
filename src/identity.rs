//! Identity key management for the DVM.
//!
//! Handles loading and generating the DVM's identity keypair.
//! The identity is stored as a 64-character hex private key.

use nostr_sdk::{Keys, ToBech32};
use std::path::{Path, PathBuf};
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

fn load_identity(path: &Path) -> Result<Keys, IdentityError> {
    let hex_key = std::fs::read_to_string(path)?.trim().to_string();

    Keys::parse(&hex_key).map_err(|e| IdentityError::InvalidKey(e.to_string()))
}

fn generate_and_save_identity(path: &Path) -> Result<Keys, IdentityError> {
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

    tracing::info!(
        "Generated new identity: {}",
        keys.public_key().to_bech32().unwrap_or_default()
    );

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Helper to load or generate identity using a specific data directory
    fn load_or_generate_identity_in_dir(data_dir: &std::path::Path) -> Result<Keys, IdentityError> {
        let key_path = data_dir.join("dvm-video").join("identity.key");

        if key_path.exists() {
            load_identity(&key_path)
        } else {
            generate_and_save_identity(&key_path)
        }
    }

    #[test]
    fn test_generate_new_identity() {
        let dir = tempdir().unwrap();

        let _keys = load_or_generate_identity_in_dir(dir.path()).unwrap();

        // Verify key file was created
        let key_path = dir.path().join("dvm-video").join("identity.key");
        assert!(key_path.exists());

        // Verify content is valid hex
        let content = std::fs::read_to_string(&key_path).unwrap();
        assert_eq!(content.len(), 64);
        assert!(content.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_load_existing_identity() {
        let dir = tempdir().unwrap();

        // Generate first
        let keys1 = load_or_generate_identity_in_dir(dir.path()).unwrap();

        // Load again - should get same key
        let keys2 = load_or_generate_identity_in_dir(dir.path()).unwrap();

        assert_eq!(keys1.public_key(), keys2.public_key());
    }

    #[test]
    fn test_invalid_key_format() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("dvm-video");
        std::fs::create_dir_all(&key_path).unwrap();
        std::fs::write(key_path.join("identity.key"), "invalid-key").unwrap();

        let result = load_or_generate_identity_in_dir(dir.path());
        assert!(result.is_err());
    }
}
