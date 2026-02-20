use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::config::Config;
use crate::dvm::events::DVM_VIDEO_TRANSFORM_REQUEST_KIND;
use crate::dvm_state::SharedDvmState;
use crate::nostr::EventPublisher;
use crate::video::HwAccel;

/// NIP-89 DVM Announcement kind (31990)
pub const DVM_ANNOUNCEMENT_KIND: Kind = Kind::Custom(31990);

/// NIP-65 Relay List Metadata kind (10002)
pub const RELAY_LIST_KIND: Kind = Kind::Custom(10002);

/// Index relays that aggregate relay lists (published to in addition to DVM relays)
const INDEX_RELAYS: &[&str] = &["wss://purplepag.es"];

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

/// Builds a NIP-65 relay list metadata event (kind 10002)
pub fn build_relay_list_event(config: &Config) -> EventBuilder {
    let tags: Vec<Tag> = config
        .nostr_relays
        .iter()
        .map(|url| Tag::custom(TagKind::Custom("r".into()), vec![url.to_string()]))
        .collect();

    EventBuilder::new(RELAY_LIST_KIND, "", tags)
}

/// Manages periodic DVM announcement publishing.
///
/// Republishes whenever the config changes (via admin commands)
/// or on a regular hourly interval.
pub struct AnnouncementPublisher {
    config: Arc<Config>,
    state: SharedDvmState,
    publisher: Arc<EventPublisher>,
    hwaccel: HwAccel,
    config_notify: Arc<Notify>,
}

impl AnnouncementPublisher {
    pub fn new(
        config: Arc<Config>,
        state: SharedDvmState,
        publisher: Arc<EventPublisher>,
        hwaccel: HwAccel,
        config_notify: Arc<Notify>,
    ) -> Self {
        Self {
            config,
            state,
            publisher,
            hwaccel,
            config_notify,
        }
    }

    /// Run the announcement publisher, publishing immediately and then periodically.
    ///
    /// Also republishes immediately when notified of config changes.
    pub async fn run(&self) {
        info!("Announcement publisher started");

        // Give relays a few seconds to connect before the first announcement
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Initial publish: announcement + relay list
        let config = self.current_snapshot().await;
        self.publish_announcement(&config).await;
        let mut last_relays = self.publish_relay_list(&config).await;

        // Then publish every hour or when config changes
        let mut ticker = interval(Duration::from_secs(3600));
        ticker.tick().await; // Skip the immediate tick (we already published)

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let config = self.current_snapshot().await;
                    self.publish_announcement(&config).await;
                }
                _ = self.config_notify.notified() => {
                    info!("Config changed, republishing announcement");
                    let config = self.current_snapshot().await;
                    self.publish_announcement(&config).await;

                    // Only republish relay list if relays actually changed
                    let current_relays: HashSet<String> = config.nostr_relays.iter().map(|u| u.to_string()).collect();
                    if current_relays != last_relays {
                        info!("Relay list changed, republishing NIP-65");
                        last_relays = self.publish_relay_list(&config).await;
                    }

                    // Reset the interval so we don't publish again too soon
                    ticker.reset();
                }
            }
        }
    }

    /// Build a current Config snapshot from the shared DVM state.
    fn current_config(&self, state: &crate::dvm_state::DvmState) -> Config {
        Config {
            nostr_keys: self.config.nostr_keys.clone(),
            nostr_relays: state
                .config
                .relays
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
            blossom_servers: state
                .config
                .blossom_servers
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
            blob_expiration_days: state.config.blob_expiration_days,
            temp_dir: self.config.temp_dir.clone(),
            ffmpeg_path: self.config.ffmpeg_path.clone(),
            ffprobe_path: self.config.ffprobe_path.clone(),
            http_port: self.config.http_port,
            http_enabled: self.config.http_enabled,
            dvm_name: state.config.name.clone(),
            dvm_about: state.config.about.clone(),
            admin_pubkey: state.config.admin.clone(),
        }
    }

    async fn current_snapshot(&self) -> Config {
        let state = self.state.read().await;
        self.current_config(&state)
    }

    async fn publish_announcement(&self, config: &Config) {
        let name = config
            .dvm_name
            .clone()
            .unwrap_or_else(|| DEFAULT_DVM_NAME.to_string());

        info!(
            name = %name,
            about = ?config.dvm_about,
            "Publishing DVM announcement"
        );

        let event = build_announcement_event(config, self.hwaccel);

        match self.publisher.publish(event).await {
            Ok(_) => {
                info!(
                    pubkey = %config.nostr_keys.public_key(),
                    service_id = %DVM_SERVICE_ID,
                    "DVM announcement published"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to publish DVM announcement");
            }
        }
    }

    /// Publish NIP-65 relay list. Returns the set of relays that were published.
    async fn publish_relay_list(&self, config: &Config) -> HashSet<String> {
        let relay_list = build_relay_list_event(config);

        // Collect DVM relay URLs + index relay URLs
        let mut relay_urls: Vec<String> = config.nostr_relays.iter().map(|u| u.to_string()).collect();
        for index_relay in INDEX_RELAYS {
            let s = index_relay.to_string();
            if !relay_urls.iter().any(|existing| existing.trim_end_matches('/') == s) {
                relay_urls.push(s);
            }
        }

        // Ensure index relays are connected
        let index_urls: Vec<::url::Url> = INDEX_RELAYS
            .iter()
            .filter_map(|s| ::url::Url::parse(s).ok())
            .collect();
        self.publisher.ensure_relays_connected(&index_urls).await;

        let published_relays: HashSet<String> = config.nostr_relays.iter().map(|u| u.to_string()).collect();

        match self.publisher.publish_to(relay_list, &relay_urls).await {
            Ok(_) => {
                info!(
                    relays = ?config.nostr_relays.iter().map(|u| u.as_str()).collect::<Vec<_>>(),
                    index_relays = ?INDEX_RELAYS,
                    "NIP-65 relay list published"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to publish relay list");
            }
        }

        published_relays
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
            http_enabled: true,
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
            http_enabled: true,
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
