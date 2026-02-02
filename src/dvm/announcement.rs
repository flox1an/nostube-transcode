use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::config::Config;
use crate::dvm::events::DVM_VIDEO_TRANSFORM_REQUEST_KIND;
use crate::nostr::EventPublisher;
use crate::video::HwAccel;

/// NIP-89 DVM Announcement kind (31990)
pub const DVM_ANNOUNCEMENT_KIND: Kind = Kind::Custom(31990);

/// DVM service identifier for video transformation
pub const DVM_SERVICE_ID: &str = "video-transform-hls";

/// Default DVM name if not configured
const DEFAULT_DVM_NAME: &str = "Video Transform DVM";

/// Builds a NIP-89 DVM announcement event
pub fn build_announcement_event(config: &Config, hwaccel: HwAccel) -> EventBuilder {
    let relays: Vec<String> = config.nostr_relays.iter().map(|u| u.to_string()).collect();

    // Use configured name or default
    let name = config
        .dvm_name
        .clone()
        .unwrap_or_else(|| DEFAULT_DVM_NAME.to_string());

    // Use configured about or build default
    let about = config.dvm_about.clone().unwrap_or_else(|| {
        format!(
            "Video transformation DVM - converts videos to HLS streaming format. \
             Supports 360p, 720p, 1080p, and 4K. Hardware acceleration: {}.",
            hwaccel
        )
    });

    // Expiration: 1 hour from now
    let expiration = Timestamp::now() + Duration::from_secs(3600);

    let mut tags = vec![
        // NIP-40 expiration tag
        Tag::expiration(expiration),
        // NIP-89 required tags
        Tag::custom(
            TagKind::Custom("d".into()),
            vec![DVM_SERVICE_ID.to_string()],
        ),
        Tag::custom(
            TagKind::Custom("k".into()),
            vec![DVM_VIDEO_TRANSFORM_REQUEST_KIND.as_u16().to_string()],
        ),
        // Service metadata
        Tag::custom(TagKind::Custom("name".into()), vec![name]),
        Tag::custom(TagKind::Custom("about".into()), vec![about]),
        // Supported input/output
        Tag::custom(
            TagKind::Custom("encryption".into()),
            vec!["nip04".to_string()],
        ),
        // Relay hints for clients
        Tag::custom(TagKind::Custom("relays".into()), relays),
    ];

    // Add supported output modes
    tags.push(Tag::custom(
        TagKind::Custom("param".into()),
        vec!["mode".to_string(), "hls".to_string(), "mp4".to_string()],
    ));

    // Add supported resolutions
    tags.push(Tag::custom(
        TagKind::Custom("param".into()),
        vec![
            "resolution".to_string(),
            "360p".to_string(),
            "480p".to_string(),
            "720p".to_string(),
            "1080p".to_string(),
        ],
    ));

    // Add admin/operator tag if configured (NIP-89)
    if let Some(admin) = &config.admin_pubkey {
        tags.push(Tag::custom(
            TagKind::Custom("admin".into()),
            vec![admin.clone()],
        ));
        
        // Also add a p tag with "operator" marker for admin dashboard discovery
        // Format: ["p", "<pubkey>", "", "operator"]
        if let Ok(pubkey) = PublicKey::parse(admin) {
            tags.push(Tag::custom(
                TagKind::Custom("p".into()),
                vec![pubkey.to_string(), "".to_string(), "operator".to_string()],
            ));
        }
    }

    EventBuilder::new(DVM_ANNOUNCEMENT_KIND, "", tags)
}

/// Manages periodic DVM announcement publishing
pub struct AnnouncementPublisher {
    config: Arc<Config>,
    publisher: Arc<EventPublisher>,
    hwaccel: HwAccel,
}

impl AnnouncementPublisher {
    pub fn new(config: Arc<Config>, publisher: Arc<EventPublisher>, hwaccel: HwAccel) -> Self {
        Self {
            config,
            publisher,
            hwaccel,
        }
    }

    /// Run the announcement publisher, publishing immediately and then periodically
    pub async fn run(&self) {
        info!("Announcement publisher started");

        // Publish immediately on startup
        self.publish_announcement().await;

        // Then publish every hour
        let mut ticker = interval(Duration::from_secs(3600));
        ticker.tick().await; // Skip the immediate tick (we already published)

        loop {
            ticker.tick().await;
            self.publish_announcement().await;
        }
    }

    async fn publish_announcement(&self) {
        let name = self
            .config
            .dvm_name
            .clone()
            .unwrap_or_else(|| DEFAULT_DVM_NAME.to_string());

        info!(
            name = %name,
            about = ?self.config.dvm_about,
            "Publishing DVM announcement"
        );

        let event = build_announcement_event(&self.config, self.hwaccel);

        match self.publisher.publish(event).await {
            Ok(_) => {
                info!(
                    pubkey = %self.config.nostr_keys.public_key(),
                    service_id = %DVM_SERVICE_ID,
                    "DVM announcement published"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to publish DVM announcement");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_announcement_event_kind() {
        assert_eq!(DVM_ANNOUNCEMENT_KIND.as_u16(), 31990);
    }

    #[test]
    fn test_announcement_includes_admin_tag() {
        let keys = Keys::generate();
        let admin_pubkey = "b7c6f6915cfa9a62fff6a1f02604de88c23c6c6c6d1b8f62c7cc10749f307e81";
        
        let config = Config {
            nostr_keys: keys.clone(),
            nostr_relays: vec![],
            blossom_servers: vec![],
            blob_expiration_days: 30,
            temp_dir: PathBuf::from("/tmp"),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            ffprobe_path: PathBuf::from("ffprobe"),
            http_port: 3000,
            dvm_name: Some("Test DVM".to_string()),
            dvm_about: Some("Test DVM about".to_string()),
            admin_pubkey: Some(admin_pubkey.to_string()),
        };

        let event_builder = build_announcement_event(&config, HwAccel::Software);
        let event = event_builder.to_event(&keys).unwrap();

        // Find the admin tag
        let admin_tag = event.tags.iter().find(|tag| {
            tag.as_slice().first().map(|s| s.as_str()) == Some("admin")
        });

        assert!(admin_tag.is_some(), "Admin tag should be present");
        let admin_value = admin_tag.unwrap().as_slice().get(1).unwrap();
        assert_eq!(admin_value, admin_pubkey);
        
        // Find the p tag with operator marker
        let p_tag = event.tags.iter().find(|tag| {
            tag.as_slice().first().map(|s| s.as_str()) == Some("p")
        });

        assert!(p_tag.is_some(), "p tag should be present");
        let tag_slice = p_tag.unwrap().as_slice();
        let p_value = tag_slice.get(1).unwrap();
        assert_eq!(p_value, admin_pubkey);
        
        // Check for "operator" marker at index 3
        let operator_marker = tag_slice.get(3);
        assert_eq!(operator_marker.map(|s| s.as_str()), Some("operator"), "p tag should have 'operator' marker");
    }

    #[test]
    fn test_announcement_without_admin_tag() {
        let keys = Keys::generate();
        
        let config = Config {
            nostr_keys: keys.clone(),
            nostr_relays: vec![],
            blossom_servers: vec![],
            blob_expiration_days: 30,
            temp_dir: PathBuf::from("/tmp"),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            ffprobe_path: PathBuf::from("ffprobe"),
            http_port: 3000,
            dvm_name: Some("Test DVM".to_string()),
            dvm_about: Some("Test DVM about".to_string()),
            admin_pubkey: None,
        };

        let event_builder = build_announcement_event(&config, HwAccel::Software);
        let event = event_builder.to_event(&keys).unwrap();

        // Find the admin tag
        let admin_tag = event.tags.iter().find(|tag| {
            tag.as_slice().first().map(|s| s.as_str()) == Some("admin")
        });

        assert!(admin_tag.is_none(), "Admin tag should not be present when no admin is configured");
    }
}
