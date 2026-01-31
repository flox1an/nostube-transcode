//! Pairing module for DVM remote configuration.
//!
//! Provides secure pairing functionality using short-lived secrets
//! and QR codes for easy mobile pairing.

use nostr_sdk::prelude::*;
use qrcode::render::unicode;
use qrcode::QrCode;
use std::time::{Duration, Instant};

/// Length of the secret (without dashes): 12 characters
const SECRET_LENGTH: usize = 12;

/// Character set for secrets - excludes confusing characters (0, 1, i, l, o)
const SECRET_CHARSET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";

/// Pairing timeout: 5 minutes
const PAIRING_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Represents an active pairing session with a short-lived secret.
pub struct PairingState {
    /// The pairing secret in xxxx-xxxx-xxxx format
    secret: String,
    /// When this pairing session was created
    created_at: Instant,
    /// The DVM's public key
    dvm_pubkey: PublicKey,
}

impl PairingState {
    /// Creates a new pairing state with a fresh secret.
    pub fn new(dvm_pubkey: PublicKey) -> Self {
        Self {
            secret: generate_pairing_secret(),
            created_at: Instant::now(),
            dvm_pubkey,
        }
    }

    /// Checks if this pairing session is still valid (not expired).
    pub fn is_valid(&self) -> bool {
        self.created_at.elapsed() < PAIRING_TIMEOUT
    }

    /// Verifies a provided secret against this pairing state.
    ///
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn verify(&self, provided: &str) -> bool {
        if !self.is_valid() {
            return false;
        }
        constant_time_eq(self.secret.as_bytes(), provided.as_bytes())
    }

    /// Returns the pairing URL for this session.
    ///
    /// Format: `{base_url}/pair?dvm={npub}&secret={secret}`
    pub fn pairing_url(&self, base_url: &str) -> String {
        let npub = self.dvm_pubkey.to_bech32().unwrap_or_default();
        format!(
            "{}/pair?dvm={}&secret={}",
            base_url.trim_end_matches('/'),
            npub,
            self.secret
        )
    }

    /// Displays the pairing QR code to the console.
    pub fn display(&self, base_url: &str) {
        let url = self.pairing_url(base_url);

        match QrCode::new(&url) {
            Ok(code) => {
                let qr_string = code
                    .render::<unicode::Dense1x2>()
                    .dark_color(unicode::Dense1x2::Light)
                    .light_color(unicode::Dense1x2::Dark)
                    .build();

                println!("\n=== DVM Pairing ===\n");
                println!("{}", qr_string);
                println!("\nPairing URL: {}", url);
                println!("Secret: {}", self.secret);
                println!("\nThis pairing code expires in 5 minutes.\n");
            }
            Err(e) => {
                eprintln!("Failed to generate QR code: {}", e);
                println!("\nPairing URL: {}", url);
                println!("Secret: {}", self.secret);
            }
        }
    }

    /// Returns the secret for testing purposes.
    #[cfg(test)]
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

/// Generates a pairing secret in xxxx-xxxx-xxxx format.
///
/// Uses a character set that excludes confusing characters (0, 1, i, l, o).
fn generate_pairing_secret() -> String {
    use ::rand::Rng;
    let mut rng = ::rand::rng();
    let mut chars = Vec::with_capacity(SECRET_LENGTH);

    for _ in 0..SECRET_LENGTH {
        let idx = rng.random_range(0..SECRET_CHARSET.len());
        chars.push(SECRET_CHARSET[idx] as char);
    }

    // Format as xxxx-xxxx-xxxx
    format!(
        "{}-{}-{}",
        chars[0..4].iter().collect::<String>(),
        chars[4..8].iter().collect::<String>(),
        chars[8..12].iter().collect::<String>()
    )
}

/// Performs constant-time comparison of two byte slices.
///
/// This prevents timing attacks by ensuring the comparison takes
/// the same amount of time regardless of where the first difference occurs.
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

        // Should be 14 characters: xxxx-xxxx-xxxx
        assert_eq!(secret.len(), 14);

        // Should have dashes at positions 4 and 9
        let chars: Vec<char> = secret.chars().collect();
        assert_eq!(chars[4], '-');
        assert_eq!(chars[9], '-');

        // All other characters should be from the allowed charset
        for (i, c) in chars.iter().enumerate() {
            if i == 4 || i == 9 {
                continue;
            }
            assert!(
                SECRET_CHARSET.contains(&(*c as u8)),
                "Character '{}' not in allowed charset",
                c
            );
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

        // Should be valid immediately
        assert!(state.is_valid());

        // Should verify with correct secret
        let secret = state.secret().to_string();
        assert!(state.verify(&secret));

        // Should not verify with wrong secret
        assert!(!state.verify("wrong-secr-etxx"));
    }

    #[test]
    fn test_pairing_url() {
        let keys = Keys::generate();
        let state = PairingState::new(keys.public_key());

        let url = state.pairing_url("https://example.com");
        let npub = keys.public_key().to_bech32().unwrap();

        // Should contain base URL
        assert!(url.starts_with("https://example.com/pair?"));

        // Should contain dvm parameter with npub
        assert!(url.contains(&format!("dvm={}", npub)));

        // Should contain secret parameter
        assert!(url.contains(&format!("secret={}", state.secret())));

        // Test with trailing slash
        let url2 = state.pairing_url("https://example.com/");
        assert!(url2.starts_with("https://example.com/pair?"));
    }

    #[test]
    fn test_constant_time_eq() {
        // Equal slices
        assert!(constant_time_eq(b"hello", b"hello"));

        // Different lengths
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"hell", b"hello"));

        // Same length, different content
        assert!(!constant_time_eq(b"hello", b"hella"));
        assert!(!constant_time_eq(b"hello", b"jello"));

        // Empty slices
        assert!(constant_time_eq(b"", b""));
    }
}
