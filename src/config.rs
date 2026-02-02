use nostr_sdk::Keys;
use std::path::PathBuf;
use url::Url;

use crate::error::ConfigError;
use crate::remote_config::RemoteConfig;

#[derive(Debug, Clone)]
pub struct Config {
    pub nostr_keys: Keys,
    pub nostr_relays: Vec<Url>,
    pub blossom_servers: Vec<Url>,
    pub blob_expiration_days: u32,
    pub temp_dir: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub http_port: u16,
    pub dvm_name: Option<String>,
    pub dvm_about: Option<String>,
    pub admin_pubkey: Option<String>,
}

impl Config {
    /// Create Config from RemoteConfig
    pub fn from_remote(
        keys: Keys,
        remote: &RemoteConfig,
        ffmpeg_path: PathBuf,
        ffprobe_path: PathBuf,
    ) -> Result<Self, ConfigError> {
        let relays: Vec<Url> = remote
            .relays
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();

        let blossom: Vec<Url> = remote
            .blossom_servers
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();

        let temp_dir = std::env::var("TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./temp"));

        let http_port = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3000);

        Ok(Self {
            nostr_keys: keys,
            nostr_relays: relays,
            blossom_servers: blossom,
            blob_expiration_days: remote.blob_expiration_days,
            temp_dir,
            ffmpeg_path,
            ffprobe_path,
            http_port,
            dvm_name: remote.name.clone(),
            dvm_about: remote.about.clone(),
            admin_pubkey: remote.admin.clone(),
        })
    }
}
