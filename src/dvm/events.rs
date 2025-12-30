use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::DvmError;

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
}

/// Result of a DVM job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DvmResult {
    Mp4(Mp4Result),
    Hls(HlsResult),
}

impl JobContext {
    pub fn from_event(event: Event) -> Result<Self, DvmError> {
        let input = Self::extract_input(&event)?;
        let relays = Self::extract_relays(&event);
        let (mode, resolution) = Self::extract_params(&event);

        Ok(Self {
            request: event,
            was_encrypted: false,
            input,
            relays,
            mode,
            resolution,
        })
    }

    fn extract_params(event: &Event) -> (OutputMode, Resolution) {
        let mut mode = OutputMode::default();
        let mut resolution = Resolution::default();

        for tag in event.tags.iter() {
            let parts: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if parts.first() == Some(&"param") && parts.len() >= 3 {
                match parts[1] {
                    "mode" => mode = OutputMode::from_str(parts[2]),
                    "resolution" => resolution = Resolution::from_str(parts[2]),
                    _ => {}
                }
            }
        }

        (mode, resolution)
    }

    fn extract_input(event: &Event) -> Result<DvmInput, DvmError> {
        let tag = event
            .tags
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

    fn extract_relays(event: &Event) -> Vec<::url::Url> {
        event
            .tags
            .iter()
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
    build_status_event_with_eta(job_id, requester, status, message, None)
}

/// Build a status event with optional estimated time remaining
pub fn build_status_event_with_eta(
    job_id: EventId,
    requester: PublicKey,
    status: JobStatus,
    message: Option<&str>,
    remaining_secs: Option<u64>,
) -> EventBuilder {
    let mut tags = vec![
        Tag::event(job_id),
        Tag::public_key(requester),
        Tag::custom(
            TagKind::Custom("status".into()),
            vec![status.as_str().to_string()],
        ),
    ];

    if let Some(msg) = message {
        tags.push(Tag::custom(
            TagKind::Custom("content".into()),
            vec![msg.to_string()],
        ));
    }

    if let Some(secs) = remaining_secs {
        tags.push(Tag::custom(
            TagKind::Custom("eta".into()),
            vec![secs.to_string()],
        ));
    }

    EventBuilder::new(DVM_STATUS_KIND, "", tags)
}

/// Build a result event for a completed job
pub fn build_result_event(
    job_id: EventId,
    requester: PublicKey,
    result: &DvmResult,
) -> EventBuilder {
    let tags = vec![Tag::event(job_id), Tag::public_key(requester)];

    // NIP-90: output goes in content field as JSON
    let content = serde_json::to_string(result).unwrap_or_default();

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
}
