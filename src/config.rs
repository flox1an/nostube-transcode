use nostr_sdk::Keys;
use std::path::PathBuf;
use url::Url;

use crate::error::ConfigError;
use crate::util::FfmpegPaths;

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
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let private_key = std::env::var("NOSTR_PRIVATE_KEY")
            .map_err(|_| ConfigError::Missing("NOSTR_PRIVATE_KEY"))?;

        let nostr_keys = Keys::parse(&private_key)
            .map_err(|e| ConfigError::InvalidKey(e.to_string()))?;

        let nostr_relays = std::env::var("NOSTR_RELAYS")
            .map_err(|_| ConfigError::Missing("NOSTR_RELAYS"))?
            .split(',')
            .map(|s| Url::parse(s.trim()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ConfigError::InvalidUrl(e.to_string()))?;

        let blossom_servers = std::env::var("BLOSSOM_UPLOAD_SERVERS")
            .map_err(|_| ConfigError::Missing("BLOSSOM_UPLOAD_SERVERS"))?
            .split(',')
            .map(|s| Url::parse(s.trim()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ConfigError::InvalidUrl(e.to_string()))?;

        let blob_expiration_days = std::env::var("BLOSSOM_BLOB_EXPIRATION_DAYS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidValue("BLOSSOM_BLOB_EXPIRATION_DAYS"))?;

        let temp_dir = std::env::var("TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./temp"));

        // Use FFmpeg discovery
        let ffmpeg_paths = FfmpegPaths::discover()?;

        let http_port = std::env::var("HTTP_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidValue("HTTP_PORT"))?;

        let dvm_name = std::env::var("DVM_NAME").ok();
        let dvm_about = std::env::var("DVM_ABOUT").ok();

        Ok(Self {
            nostr_keys,
            nostr_relays,
            blossom_servers,
            blob_expiration_days,
            temp_dir,
            ffmpeg_path: ffmpeg_paths.ffmpeg,
            ffprobe_path: ffmpeg_paths.ffprobe,
            http_port,
            dvm_name,
            dvm_about,
        })
    }
}
