use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use nostr_sdk::prelude::*;

use crate::dvm::BLOSSOM_AUTH_KIND;
use crate::error::BlossomError;

/// Create a Blossom upload authorization token
pub fn create_upload_auth_token(
    keys: &Keys,
    size: u64,
    sha256: &str,
) -> Result<String, BlossomError> {
    let now = Timestamp::now();
    let expiration = Timestamp::from(now.as_u64() + 600); // +10 min

    let tags = vec![
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::T)),
            vec!["upload"],
        ),
        Tag::custom(TagKind::Custom("size".into()), vec![size.to_string()]),
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::X)),
            vec![sha256.to_string()],
        ),
        Tag::custom(TagKind::Custom("name".into()), vec![sha256.to_string()]),
        Tag::expiration(expiration),
    ];

    let event = EventBuilder::new(BLOSSOM_AUTH_KIND, "Upload", tags)
        .to_event(keys)
        .map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    let json =
        serde_json::to_string(&event).map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    Ok(STANDARD.encode(json))
}

/// Create a Blossom delete authorization token
pub fn create_delete_auth_token(keys: &Keys, sha256: &str) -> Result<String, BlossomError> {
    let now = Timestamp::now();
    let expiration = Timestamp::from(now.as_u64() + 600); // +10 min

    let tags = vec![
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::T)),
            vec!["delete"],
        ),
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::X)),
            vec![sha256.to_string()],
        ),
        Tag::expiration(expiration),
    ];

    let event = EventBuilder::new(BLOSSOM_AUTH_KIND, "Delete", tags)
        .to_event(keys)
        .map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    let json =
        serde_json::to_string(&event).map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    Ok(STANDARD.encode(json))
}

/// Create a Blossom list authorization token
pub fn create_list_auth_token(keys: &Keys) -> Result<String, BlossomError> {
    let now = Timestamp::now();
    let expiration = Timestamp::from(now.as_u64() + 600); // +10 min

    let tags = vec![
        Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::T)),
            vec!["list"],
        ),
        Tag::expiration(expiration),
    ];

    let event = EventBuilder::new(BLOSSOM_AUTH_KIND, "List Blobs", tags)
        .to_event(keys)
        .map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    let json =
        serde_json::to_string(&event).map_err(|e| BlossomError::AuthFailed(e.to_string()))?;

    Ok(STANDARD.encode(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_upload_auth_token() {
        let keys = Keys::generate();
        let token = create_upload_auth_token(&keys, 1024, "abc123").unwrap();

        // Token should be base64 encoded
        assert!(!token.is_empty());
        let decoded = STANDARD.decode(&token).unwrap();
        let json = String::from_utf8(decoded).unwrap();

        // Should be valid JSON
        let event: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(event["kind"], 24242);
    }
}
