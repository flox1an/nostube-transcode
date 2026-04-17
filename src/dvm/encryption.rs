use nostr_sdk::prelude::*;

use crate::error::DvmError;

/// Tracks which encryption the client used, so we reply with the same type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionType {
    None,
    Nip04,
    Nip44,
}

impl EncryptionType {
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, EncryptionType::None)
    }
}

/// Decrypt encrypted content from a DVM request (NIP-04 or NIP-44)
pub async fn decrypt_content(
    keys: &Keys,
    sender: &PublicKey,
    encrypted: &str,
) -> Result<String, DvmError> {
    let decrypted = nip04::decrypt(keys.secret_key(), sender, encrypted)
        .or_else(|_| nip44::decrypt(keys.secret_key(), sender, encrypted))
        .map_err(|e| DvmError::JobRejected(format!("Decryption failed: {}", e)))?;

    Ok(decrypted)
}

/// Encrypt content for a DVM response, matching the client's encryption type.
pub fn encrypt_for_dvm(
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
    enc_type: EncryptionType,
) -> Result<String, DvmError> {
    match enc_type {
        EncryptionType::None => Ok(content.to_string()),
        EncryptionType::Nip04 => nip04::encrypt(keys.secret_key(), recipient, content)
            .map_err(|e| DvmError::JobRejected(format!("NIP-04 encryption failed: {}", e))),
        EncryptionType::Nip44 => nip44::encrypt(
            keys.secret_key(),
            recipient,
            content,
            nip44::Version::default(),
        )
        .map_err(|e| DvmError::JobRejected(format!("NIP-44 encryption failed: {}", e))),
    }
}

/// Encrypt NIP-04 content for a DVM response (legacy helper)
pub async fn encrypt_content(
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<String, DvmError> {
    let encrypted = nip04::encrypt(keys.secret_key(), recipient, content)
        .map_err(|e| DvmError::JobRejected(format!("Encryption failed: {}", e)))?;

    Ok(encrypted)
}

/// Check if event content appears to be encrypted (NIP-04 or NIP-44)
pub fn is_encrypted(content: &str) -> bool {
    // NIP-04: base64?iv=base64
    if content.contains("?iv=") {
        return true;
    }
    // Not plaintext JSON or a URL — likely NIP-44 encrypted blob
    !content.is_empty() && !content.starts_with('{') && !content.starts_with("http")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_encrypted() {
        // NIP-04
        assert!(is_encrypted("somebase64content?iv=someivbase64"));
        // NIP-44 (base64 blob)
        assert!(is_encrypted("ArY5lBmMCe3vdcKqFGEFVxph0MnAMj7R3x5mu"));
        // Plaintext (JSON content or URLs)
        assert!(!is_encrypted("https://example.com/video.mp4"));
        assert!(!is_encrypted("{\"i\":[]}"));
        assert!(!is_encrypted(""));
    }

    #[test]
    fn test_encryption_type_is_encrypted() {
        assert!(!EncryptionType::None.is_encrypted());
        assert!(EncryptionType::Nip04.is_encrypted());
        assert!(EncryptionType::Nip44.is_encrypted());
    }
}
