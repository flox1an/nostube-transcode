use nostr_sdk::prelude::*;

use crate::error::DvmError;

/// Decrypt NIP-04 encrypted content from a DVM request
pub async fn decrypt_content(
    keys: &Keys,
    sender: &PublicKey,
    encrypted: &str,
) -> Result<String, DvmError> {
    let decrypted = nip04::decrypt(keys.secret_key(), sender, encrypted)
        .map_err(|e| DvmError::JobRejected(format!("Decryption failed: {}", e)))?;

    Ok(decrypted)
}

/// Encrypt NIP-04 content for a DVM response
pub async fn encrypt_content(
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<String, DvmError> {
    let encrypted = nip04::encrypt(keys.secret_key(), recipient, content)
        .map_err(|e| DvmError::JobRejected(format!("Encryption failed: {}", e)))?;

    Ok(encrypted)
}

/// Check if event content appears to be NIP-04 encrypted
pub fn is_encrypted(content: &str) -> bool {
    // NIP-04 encrypted content has a specific format: base64?iv=base64
    content.contains("?iv=")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_encrypted() {
        assert!(is_encrypted("somebase64content?iv=someivbase64"));
        assert!(!is_encrypted("plain text content"));
        assert!(!is_encrypted("https://example.com/video.mp4"));
    }
}
