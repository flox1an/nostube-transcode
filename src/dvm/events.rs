use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

use crate::dvm::encryption::is_encrypted;
use crate::error::DvmError;

/// Expiration time for status events (1 hour)
const STATUS_EXPIRATION_SECS: u64 = 3600;

/// Expiration time for result events (1 hour)
const RESULT_EXPIRATION_SECS: u64 = 3600;

pub const DVM_STATUS_KIND: Kind = Kind::Custom(7000);
pub const DVM_VIDEO_TRANSFORM_REQUEST_KIND: Kind = Kind::Custom(5207);
pub const DVM_VIDEO_TRANSFORM_RESULT_KIND: Kind = Kind::Custom(6207);
pub const BLOSSOM_AUTH_KIND: Kind = Kind::Custom(24242);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    #[default]
    Mp4,
    Hls,
}

impl OutputMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hls" => Self::Hls,
            _ => Self::Mp4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codec {
    #[default]
    H264,
    H265,
}

impl Codec {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "h265" | "hevc" => Self::H265,
            _ => Self::H264,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    R240p,
    R360p,
    R480p,
    #[default]
    R720p,
    R1080p,
}

impl Resolution {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "240p" => Self::R240p,
            "360p" => Self::R360p,
            "480p" => Self::R480p,
            "1080p" => Self::R1080p,
            _ => Self::R720p,
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::R240p => (426, 240),
            Self::R360p => (640, 360),
            Self::R480p => (854, 480),
            Self::R720p => (1280, 720),
            Self::R1080p => (1920, 1080),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::R240p => "240p",
            Self::R360p => "360p",
            Self::R480p => "480p",
            Self::R720p => "720p",
            Self::R1080p => "1080p",
        }
    }
}

/// HLS resolution selection for adaptive streaming
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HlsResolution {
    R240p,
    R360p,
    R480p,
    R720p,
    R1080p,
    /// Include original quality (passthrough if codec compatible, else re-encode)
    Original,
}

impl HlsResolution {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "240p" => Some(Self::R240p),
            "360p" => Some(Self::R360p),
            "480p" => Some(Self::R480p),
            "720p" => Some(Self::R720p),
            "1080p" => Some(Self::R1080p),
            "original" => Some(Self::Original),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::R240p => "240p",
            Self::R360p => "360p",
            Self::R480p => "480p",
            Self::R720p => "720p",
            Self::R1080p => "1080p",
            Self::Original => "original",
        }
    }

    /// Get the height in pixels for this resolution (None for Original)
    pub fn height(&self) -> Option<u32> {
        match self {
            Self::R240p => Some(240),
            Self::R360p => Some(360),
            Self::R480p => Some(480),
            Self::R720p => Some(720),
            Self::R1080p => Some(1080),
            Self::Original => None,
        }
    }

    /// Returns all available HLS resolutions (default selection)
    pub fn all() -> Vec<Self> {
        vec![
            Self::R240p,
            Self::R360p,
            Self::R480p,
            Self::R720p,
            Self::R1080p,
            Self::Original,
        ]
    }
}

/// Parse comma-separated HLS resolutions string
pub fn parse_hls_resolutions(s: &str) -> Vec<HlsResolution> {
    s.split(',')
        .filter_map(|r| HlsResolution::from_str(r.trim()))
        .collect()
}

#[derive(Debug, Clone)]
pub struct DvmInput {
    pub value: String,
    pub input_type: String,
    pub relay: Option<String>,
    pub marker: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobContext {
    pub request: Event,
    pub was_encrypted: bool,
    pub input: DvmInput,
    pub relays: Vec<::url::Url>,
    pub mode: OutputMode,
    pub resolution: Resolution,
    pub codec: Codec,
    /// Selected resolutions for HLS mode (empty means use all)
    pub hls_resolutions: Vec<HlsResolution>,
    /// Enable AES-128 encryption for HLS (defaults to true for backward compatibility)
    pub encryption: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    PaymentRequired,
    Processing,
    Partial,
    Success,
    Error,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaymentRequired => "payment-required",
            Self::Processing => "processing",
            Self::Partial => "partial",
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}

/// Stream playlist info for HLS output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPlaylist {
    pub url: String,
    pub resolution: String,
    /// Total size of this stream (playlist + segments) in bytes
    pub size_bytes: u64,
    /// MIME type with codecs (e.g., "video/mp4; codecs=\"hvc1,mp4a.40.2\"")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
}

/// DVM result for MP4 output - list of URLs from different servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mp4Result {
    pub urls: Vec<String>,
    pub resolution: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// MIME type with codecs (e.g., "video/mp4; codecs=\"hvc1,mp4a.40.2\"")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
}

/// DVM result for HLS output - master playlist + stream playlists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsResult {
    pub master_playlist: String,
    pub stream_playlists: Vec<StreamPlaylist>,
    /// Total size of all files in bytes
    pub total_size_bytes: u64,
    /// Base64-encoded AES-128 encryption key (if encryption is enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_key: Option<String>,
}

/// Result of a DVM job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DvmResult {
    Mp4(Mp4Result),
    Hls(HlsResult),
}

/// Encrypted content structure for NIP-90 encrypted requests
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedContent {
    /// Input tag: ["i", value, type, relay?, marker?]
    i: Vec<String>,
    /// Parameter tags: [["param", name, value], ...]
    #[serde(default)]
    params: Vec<Vec<String>>,
}

impl JobContext {
    /// Create JobContext from event, handling both encrypted and unencrypted requests
    pub fn from_event_with_keys(event: Event, keys: &Keys) -> Result<Self, DvmError> {
        // Check if event has encrypted tag and content looks encrypted
        let has_encrypted_tag = event
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("encrypted"));

        let content_is_encrypted = is_encrypted(&event.content);

        if has_encrypted_tag && content_is_encrypted {
            debug!("Processing encrypted DVM request");
            Self::from_encrypted_event(event, keys)
        } else {
            Self::from_event(event)
        }
    }

    /// Create JobContext from an unencrypted event
    pub fn from_event(event: Event) -> Result<Self, DvmError> {
        let tags: Vec<Tag> = event.tags.iter().cloned().collect();
        let input = Self::extract_input_from_tags(&tags)?;
        let relays = Self::extract_relays_from_tags(&tags);
        let (mode, resolution, codec, hls_resolutions, encryption) =
            Self::extract_params_from_tags(&tags);

        Ok(Self {
            request: event,
            was_encrypted: false,
            input,
            relays,
            mode,
            resolution,
            codec,
            hls_resolutions,
            encryption,
        })
    }

    /// Create JobContext from an encrypted event (NIP-04)
    fn from_encrypted_event(event: Event, keys: &Keys) -> Result<Self, DvmError> {
        // Decrypt content using NIP-04
        let decrypted = nip04::decrypt(keys.secret_key(), &event.pubkey, &event.content)
            .map_err(|e| DvmError::JobRejected(format!("Failed to decrypt request: {}", e)))?;

        // Parse decrypted content as JSON containing i and params
        let encrypted_content: EncryptedContent =
            serde_json::from_str(&decrypted).map_err(|e| {
                DvmError::JobRejected(format!("Invalid encrypted content format: {}", e))
            })?;

        // Extract input from decrypted content
        let input = Self::extract_input_from_vec(&encrypted_content.i)?;

        // Build virtual tags from decrypted params for parameter extraction
        let mut virtual_tags: Vec<Tag> = encrypted_content
            .params
            .iter()
            .map(|p| Tag::custom(TagKind::Custom(p[0].clone().into()), p[1..].to_vec()))
            .collect();

        // Also include unencrypted tags (relays, p tag, etc.)
        for tag in event.tags.iter() {
            let tag_name = tag.as_slice().first().map(|s| s.as_str());
            // Include relays tag from unencrypted tags
            if tag_name == Some("relays") {
                virtual_tags.push(tag.clone());
            }
        }

        let relays = Self::extract_relays_from_tags(&virtual_tags);
        let (mode, resolution, codec, hls_resolutions, encryption) =
            Self::extract_params_from_tags(&virtual_tags);

        Ok(Self {
            request: event,
            was_encrypted: true,
            input,
            relays,
            mode,
            resolution,
            codec,
            hls_resolutions,
            encryption,
        })
    }

    fn extract_params_from_tags(
        tags: &[Tag],
    ) -> (OutputMode, Resolution, Codec, Vec<HlsResolution>, bool) {
        let mut mode = OutputMode::default();
        let mut resolution = Resolution::default();
        let mut codec = Codec::default();
        let mut hls_resolutions = Vec::new();
        let mut encryption = true; // Default to true for backward compatibility

        for tag in tags.iter() {
            let parts: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if parts.first() == Some(&"param") && parts.len() >= 3 {
                match parts[1] {
                    "mode" => mode = OutputMode::from_str(parts[2]),
                    "resolution" => resolution = Resolution::from_str(parts[2]),
                    "codec" => codec = Codec::from_str(parts[2]),
                    "resolutions" => hls_resolutions = parse_hls_resolutions(parts[2]),
                    "encryption" => encryption = parts[2].to_lowercase() != "false",
                    _ => {}
                }
            }
        }

        // If no resolutions specified, use all (backward compatibility)
        if hls_resolutions.is_empty() {
            hls_resolutions = HlsResolution::all();
        }

        (mode, resolution, codec, hls_resolutions, encryption)
    }

    fn extract_input_from_tags(tags: &[Tag]) -> Result<DvmInput, DvmError> {
        let tag = tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("i"))
            .ok_or_else(|| DvmError::JobRejected("Missing input tag".into()))?;

        let parts: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();

        if parts.len() < 3 {
            return Err(DvmError::JobRejected("Invalid input tag format".into()));
        }

        Ok(DvmInput {
            value: parts[1].to_string(),
            input_type: parts[2].to_string(),
            relay: parts.get(3).map(|s| s.to_string()),
            marker: parts.get(4).map(|s| s.to_string()),
        })
    }

    /// Extract input from decrypted content's "i" array
    fn extract_input_from_vec(i: &[String]) -> Result<DvmInput, DvmError> {
        if i.len() < 2 {
            return Err(DvmError::JobRejected(
                "Invalid encrypted input format".into(),
            ));
        }

        Ok(DvmInput {
            value: i[0].clone(),
            input_type: i[1].clone(),
            relay: i.get(2).cloned(),
            marker: i.get(3).cloned(),
        })
    }

    fn extract_relays_from_tags(tags: &[Tag]) -> Vec<::url::Url> {
        tags.iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("relays"))
            .map(|t| {
                t.as_slice()[1..]
                    .iter()
                    .filter_map(|s| ::url::Url::parse(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the event ID of the original request
    pub fn event_id(&self) -> EventId {
        self.request.id
    }

    /// Get the public key of the requester
    pub fn requester(&self) -> PublicKey {
        self.request.pubkey
    }
}

/// Build a status event for a job
pub fn build_status_event(
    job_id: EventId,
    requester: PublicKey,
    status: JobStatus,
    message: Option<&str>,
) -> EventBuilder {
    build_status_event_with_eta_encrypted(job_id, requester, status, message, None, None)
}

/// Build a status event with optional estimated time remaining
pub fn build_status_event_with_eta(
    job_id: EventId,
    requester: PublicKey,
    status: JobStatus,
    message: Option<&str>,
    remaining_secs: Option<u64>,
) -> EventBuilder {
    build_status_event_with_eta_encrypted(job_id, requester, status, message, remaining_secs, None)
}

/// Build a status event with optional encryption (NIP-04)
pub fn build_status_event_with_eta_encrypted(
    job_id: EventId,
    requester: PublicKey,
    status: JobStatus,
    message: Option<&str>,
    remaining_secs: Option<u64>,
    keys: Option<&Keys>,
) -> EventBuilder {
    // NIP-40 expiration: 24 hours
    let expiration = Timestamp::now() + Duration::from_secs(STATUS_EXPIRATION_SECS);

    let mut tags = vec![
        Tag::expiration(expiration),
        Tag::event(job_id),
        Tag::public_key(requester),
        Tag::custom(
            TagKind::Custom("status".into()),
            vec![status.as_str().to_string()],
        ),
    ];

    // For encrypted responses, put status details in encrypted content
    if let Some(keys) = keys {
        // Build status content JSON
        let status_content = serde_json::json!({
            "status": status.as_str(),
            "message": message,
            "eta": remaining_secs,
        });

        // Encrypt the content
        if let Ok(encrypted) =
            nip04::encrypt(keys.secret_key(), &requester, status_content.to_string())
        {
            tags.push(Tag::custom(
                TagKind::Custom("encrypted".into()),
                Vec::<String>::new(),
            ));
            return EventBuilder::new(DVM_STATUS_KIND, encrypted, tags);
        }
    }

    // Unencrypted: put message in content field and eta in tags
    let content = if let Some(msg) = message {
        // Also keep the content tag for backward compatibility with our own docs
        tags.push(Tag::custom(
            TagKind::Custom("content".into()),
            vec![msg.to_string()],
        ));
        msg.to_string()
    } else {
        status.as_str().to_string()
    };

    if let Some(secs) = remaining_secs {
        tags.push(Tag::custom(
            TagKind::Custom("eta".into()),
            vec![secs.to_string()],
        ));
    }

    EventBuilder::new(DVM_STATUS_KIND, content, tags)
}

/// Build a result event for a completed job (unencrypted)
pub fn build_result_event(
    job_id: EventId,
    requester: PublicKey,
    result: &DvmResult,
) -> EventBuilder {
    build_result_event_encrypted(job_id, requester, result, None)
}

/// Build a result event with optional encryption (NIP-04)
pub fn build_result_event_encrypted(
    job_id: EventId,
    requester: PublicKey,
    result: &DvmResult,
    keys: Option<&Keys>,
) -> EventBuilder {
    // NIP-40 expiration: 7 days
    let expiration = Timestamp::now() + Duration::from_secs(RESULT_EXPIRATION_SECS);

    let mut tags = vec![
        Tag::expiration(expiration),
        Tag::event(job_id),
        Tag::public_key(requester),
    ];

    // NIP-90: output goes in content field as JSON
    let content = serde_json::to_string(result).unwrap_or_default();

    // Encrypt if keys provided
    if let Some(keys) = keys {
        if let Ok(encrypted) = nip04::encrypt(keys.secret_key(), &requester, &content) {
            tags.push(Tag::custom(
                TagKind::Custom("encrypted".into()),
                Vec::<String>::new(),
            ));
            return EventBuilder::new(DVM_VIDEO_TRANSFORM_RESULT_KIND, encrypted, tags);
        }
    }

    EventBuilder::new(DVM_VIDEO_TRANSFORM_RESULT_KIND, content, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_as_str() {
        assert_eq!(JobStatus::PaymentRequired.as_str(), "payment-required");
        assert_eq!(JobStatus::Processing.as_str(), "processing");
        assert_eq!(JobStatus::Success.as_str(), "success");
        assert_eq!(JobStatus::Error.as_str(), "error");
    }

    #[test]
    fn test_hls_resolution_from_str() {
        assert_eq!(HlsResolution::from_str("240p"), Some(HlsResolution::R240p));
        assert_eq!(HlsResolution::from_str("360p"), Some(HlsResolution::R360p));
        assert_eq!(HlsResolution::from_str("480p"), Some(HlsResolution::R480p));
        assert_eq!(HlsResolution::from_str("720p"), Some(HlsResolution::R720p));
        assert_eq!(
            HlsResolution::from_str("1080p"),
            Some(HlsResolution::R1080p)
        );
        assert_eq!(
            HlsResolution::from_str("original"),
            Some(HlsResolution::Original)
        );
        assert_eq!(
            HlsResolution::from_str("ORIGINAL"),
            Some(HlsResolution::Original)
        );
        assert_eq!(HlsResolution::from_str("invalid"), None);
    }

    #[test]
    fn test_hls_resolution_height() {
        assert_eq!(HlsResolution::R240p.height(), Some(240));
        assert_eq!(HlsResolution::R720p.height(), Some(720));
        assert_eq!(HlsResolution::Original.height(), None);
    }

    #[test]
    fn test_parse_hls_resolutions() {
        let resolutions = parse_hls_resolutions("240p,720p,original");
        assert_eq!(resolutions.len(), 3);
        assert!(resolutions.contains(&HlsResolution::R240p));
        assert!(resolutions.contains(&HlsResolution::R720p));
        assert!(resolutions.contains(&HlsResolution::Original));
    }

    #[test]
    fn test_parse_hls_resolutions_with_spaces() {
        let resolutions = parse_hls_resolutions("240p, 720p, 1080p");
        assert_eq!(resolutions.len(), 3);
        assert!(resolutions.contains(&HlsResolution::R240p));
        assert!(resolutions.contains(&HlsResolution::R720p));
        assert!(resolutions.contains(&HlsResolution::R1080p));
    }

    #[test]
    fn test_parse_hls_resolutions_ignores_invalid() {
        let resolutions = parse_hls_resolutions("240p,invalid,720p");
        assert_eq!(resolutions.len(), 2);
        assert!(resolutions.contains(&HlsResolution::R240p));
        assert!(resolutions.contains(&HlsResolution::R720p));
    }

    #[test]
    fn test_hls_resolution_all() {
        let all = HlsResolution::all();
        assert_eq!(all.len(), 6);
        assert!(all.contains(&HlsResolution::R240p));
        assert!(all.contains(&HlsResolution::Original));
    }
}
