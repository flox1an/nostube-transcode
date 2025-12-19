use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

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

    let mut tags = vec![
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
        Tag::custom(
            TagKind::Custom("name".into()),
            vec![name],
        ),
        Tag::custom(
            TagKind::Custom("about".into()),
            vec![about],
        ),
        // Supported input/output
        Tag::custom(
            TagKind::Custom("encryption".into()),
            vec!["nip04".to_string()],
        ),
        // Relay hints for clients
        Tag::custom(
            TagKind::Custom("relays".into()),
            relays,
        ),
    ];

    // Add supported output modes
    tags.push(Tag::custom(
        TagKind::Custom("param".into()),
        vec![
            "mode".to_string(),
            "hls".to_string(),
            "mp4".to_string(),
        ],
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

    EventBuilder::new(DVM_ANNOUNCEMENT_KIND, "", tags)
}

/// Manages periodic DVM announcement publishing
pub struct AnnouncementPublisher {
    config: Arc<Config>,
    publisher: Arc<EventPublisher>,
    hwaccel: HwAccel,
}

impl AnnouncementPublisher {
    pub fn new(
        config: Arc<Config>,
        publisher: Arc<EventPublisher>,
        hwaccel: HwAccel,
    ) -> Self {
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
        let event = build_announcement_event(&self.config, self.hwaccel);

        debug!("Publishing DVM announcement");

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

    #[test]
    fn test_announcement_event_kind() {
        assert_eq!(DVM_ANNOUNCEMENT_KIND.as_u16(), 31990);
    }
}
