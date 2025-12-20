use thiserror::Error;

#[derive(Error, Debug)]
pub enum DvmError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Nostr error: {0}")]
    Nostr(#[from] nostr_sdk::client::Error),

    #[error("Video processing error: {0}")]
    Video(#[from] VideoError),

    #[error("Blossom error: {0}")]
    Blossom(#[from] BlossomError),

    #[error("Job rejected: {0}")]
    JobRejected(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing environment variable: {0}")]
    Missing(&'static str),

    #[error("Invalid private key: {0}")]
    InvalidKey(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Invalid value for {0}")]
    InvalidValue(&'static str),

    #[error("FFmpeg not found. Searched: {0}")]
    FfmpegNotFound(String),

    #[error("FFprobe not found. Searched: {0}")]
    FfprobeNotFound(String),
}

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("FFmpeg failed: {0}")]
    FfmpegFailed(String),

    #[error("FFprobe failed: {0}")]
    FfprobeFailed(String),

    #[error("Invalid video URL: {0}")]
    InvalidUrl(String),

    #[error("Playlist parse error: {0}")]
    PlaylistParse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum BlossomError {
    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("Auth token creation failed: {0}")]
    AuthFailed(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Video processing error: {0}")]
    Video(#[from] VideoError),
}
