use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info};

use crate::config::Config;
use crate::dvm::events::{Codec, HlsResolution, Resolution};
use crate::error::VideoError;
use crate::util::TempDir;
use crate::video::ffmpeg::{FfmpegCommand, FfmpegMp4Command};
use crate::video::hwaccel::HwAccel;
use crate::video::playlist::ENCRYPTION_KEY_PLACEHOLDER_URI;

/// Generate a random 16-byte AES-128 encryption key
pub fn generate_aes_key() -> [u8; 16] {
    let mut key = [0u8; 16];
    rand::rng().fill_bytes(&mut key);
    key
}

/// Convert an AES key to base64 string
pub fn key_to_base64(key: &[u8; 16]) -> String {
    STANDARD.encode(key)
}

#[derive(Debug, Clone)]
pub struct ResolutionConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_bitrate: Option<String>,
    pub audio_bitrate: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub quality: Option<u32>,
    pub is_original: bool,
}

impl Default for ResolutionConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            video_bitrate: None,
            audio_bitrate: None,
            video_codec: None,
            audio_codec: None,
            quality: None,
            is_original: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SegmentType {
    #[default]
    Fmp4,
    MpegTs,
}

impl SegmentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fmp4 => "fmp4",
            Self::MpegTs => "mpegts",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Fmp4 => "m4s",
            Self::MpegTs => "ts",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransformConfig {
    pub resolutions: HashMap<String, ResolutionConfig>,
    pub hls_time: u32,
    pub hls_list_size: u32,
    pub segment_type: SegmentType,
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self::for_resolution(None)
    }
}

impl TransformConfig {
    /// Create a transform config based on input video height.
    /// For 4K (height >= 2160), includes 240p, 360p, 480p, 720p, 1080p (encoded), and 2160p (original).
    /// For smaller inputs, includes 240p, 360p, 480p, 720p, and original resolution.
    pub fn for_resolution(input_height: Option<u32>) -> Self {
        Self::for_resolutions(input_height, &HlsResolution::all(), None)
    }

    /// Create a transform config based on selected HLS resolutions.
    ///
    /// # Arguments
    /// * `input_height` - Height of the input video in pixels
    /// * `selected` - List of resolutions selected by the user
    /// * `source_codec` - Source video codec (for determining if passthrough is possible)
    ///
    /// # Resolution filtering
    /// - Resolutions higher than input are skipped (e.g., 1080p skipped for 720p input)
    /// - "Original" uses passthrough if source codec is HLS-compatible, else re-encodes
    pub fn for_resolutions(
        input_height: Option<u32>,
        selected: &[HlsResolution],
        source_codec: Option<&str>,
    ) -> Self {
        let mut resolutions = HashMap::new();
        let input_h = input_height.unwrap_or(1080);
        let is_4k = input_h >= 2160;

        // Check if source codec is HLS-compatible (H.264 or H.265)
        let can_passthrough = source_codec
            .map(|c| Self::is_hls_compatible_codec(c))
            .unwrap_or(true); // Assume compatible if unknown

        // Track if we need to include original
        let include_original = selected.contains(&HlsResolution::Original);

        // Add each selected resolution if it's <= input height
        for res in selected {
            match res {
                HlsResolution::R240p if input_h >= 240 => {
                    resolutions.insert(
                        "240p".to_string(),
                        ResolutionConfig {
                            // Width is auto-calculated to preserve aspect ratio
                            height: Some(240),
                            quality: Some(30),
                            audio_bitrate: Some("64k".to_string()),
                            ..Default::default()
                        },
                    );
                }
                HlsResolution::R360p if input_h >= 360 => {
                    resolutions.insert(
                        "360p".to_string(),
                        ResolutionConfig {
                            // Width is auto-calculated to preserve aspect ratio
                            height: Some(360),
                            quality: Some(28),
                            audio_bitrate: Some("96k".to_string()),
                            ..Default::default()
                        },
                    );
                }
                HlsResolution::R480p if input_h >= 480 => {
                    resolutions.insert(
                        "480p".to_string(),
                        ResolutionConfig {
                            // Width is auto-calculated to preserve aspect ratio
                            height: Some(480),
                            quality: Some(26),
                            audio_bitrate: Some("128k".to_string()),
                            ..Default::default()
                        },
                    );
                }
                HlsResolution::R720p if input_h >= 720 => {
                    resolutions.insert(
                        "720p".to_string(),
                        ResolutionConfig {
                            // Width is auto-calculated to preserve aspect ratio
                            height: Some(720),
                            quality: Some(23),
                            ..Default::default()
                        },
                    );
                }
                HlsResolution::R1080p if input_h >= 1080 => {
                    // Only add 1080p as encoded if original is also selected and we're not 4K
                    // For 4K, 1080p is always encoded; for non-4K with original, 1080p is the original
                    if is_4k || !include_original {
                        resolutions.insert(
                            "1080p".to_string(),
                            ResolutionConfig {
                                // Width is auto-calculated to preserve aspect ratio
                                height: Some(1080),
                                quality: Some(20),
                                ..Default::default()
                            },
                        );
                    }
                }
                HlsResolution::Original => {
                    // Add original at input resolution
                    let label = if is_4k { "2160p" } else { "1080p" };
                    resolutions.insert(
                        label.to_string(),
                        ResolutionConfig {
                            is_original: can_passthrough,
                            // If can't passthrough, set height for re-encoding (width auto-calculated)
                            height: if can_passthrough { None } else { Some(input_h) },
                            quality: if can_passthrough { None } else { Some(18) },
                            ..Default::default()
                        },
                    );
                }
                _ => {} // Resolution higher than input, skip
            }
        }

        Self {
            resolutions,
            hls_time: 6,
            hls_list_size: 0,
            segment_type: SegmentType::Fmp4,
        }
    }

    /// Check if a codec is compatible with HLS (can be used for passthrough)
    pub fn is_hls_compatible_codec(codec: &str) -> bool {
        let codec_lower = codec.to_lowercase();
        matches!(
            codec_lower.as_str(),
            "h264" | "avc" | "avc1" | "h265" | "hevc" | "hvc1" | "hev1"
        )
    }

    /// Returns a human-readable string of the output resolutions
    pub fn resolution_label(&self) -> String {
        let mut labels: Vec<&str> = self.resolutions.keys().map(|s| s.as_str()).collect();
        labels.sort_by(|a, b| {
            let a_num: u32 = a.trim_end_matches('p').parse().unwrap_or(0);
            let b_num: u32 = b.trim_end_matches('p').parse().unwrap_or(0);
            a_num.cmp(&b_num)
        });
        labels.join(", ")
    }
}

#[derive(Debug)]
pub struct TransformResult {
    pub master_playlist_path: PathBuf,
    pub stream_playlists: Vec<PathBuf>,
    pub segment_paths: Vec<PathBuf>,
    pub stream_sizes: Vec<u64>,
    pub temp_dir: TempDir,
    /// Base64-encoded AES-128 encryption key
    pub encryption_key: String,
}

impl TransformResult {
    /// Get all files that need to be uploaded
    pub fn all_files(&self) -> Vec<&Path> {
        let mut files: Vec<&Path> = vec![self.master_playlist_path.as_path()];
        files.extend(self.stream_playlists.iter().map(|p| p.as_path()));
        files.extend(self.segment_paths.iter().map(|p| p.as_path()));
        files
    }

    /// Cleanup temporary files
    pub async fn cleanup(self) {
        let _ = self.temp_dir.cleanup().await;
    }
}

/// Result of a single MP4 transformation
#[derive(Debug)]
pub struct Mp4TransformResult {
    pub output_path: PathBuf,
    pub temp_dir: TempDir,
}

impl Mp4TransformResult {
    /// Cleanup temporary files
    pub async fn cleanup(self) {
        let _ = self.temp_dir.cleanup().await;
    }
}

pub struct VideoProcessor {
    config: Arc<Config>,
    transform_config: TransformConfig,
    hwaccel: HwAccel,
}

impl VideoProcessor {
    pub fn new(config: Arc<Config>) -> Self {
        let hwaccel = HwAccel::detect();
        info!(hwaccel = %hwaccel, "Hardware acceleration detected");

        Self {
            config,
            transform_config: TransformConfig::default(),
            hwaccel,
        }
    }

    pub fn with_transform_config(mut self, transform_config: TransformConfig) -> Self {
        self.transform_config = transform_config;
        self
    }

    /// Get the detected hardware acceleration type
    pub fn hwaccel(&self) -> HwAccel {
        self.hwaccel
    }

    /// Transform a video URL into HLS format with resolution-aware config.
    /// If input_height is provided and >= 2160 (4K), outputs will include
    /// 360p, 720p, 1080p (encoded), and 2160p (original).
    pub async fn transform(
        &self,
        input_url: &str,
        input_height: Option<u32>,
        codec: Codec,
        progress: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
        duration: Option<f64>,
    ) -> Result<(TransformResult, TransformConfig), VideoError> {
        self.transform_with_resolutions(
            input_url,
            input_height,
            codec,
            &HlsResolution::all(),
            None,
            true,
            progress,
            duration,
        )
        .await
    }

    /// Transform a video URL into HLS format with user-selected resolutions.
    ///
    /// # Arguments
    /// * `input_url` - URL of the input video
    /// * `input_height` - Height of the input video in pixels
    /// * `codec` - Target codec (H.264 or H.265)
    /// * `selected_resolutions` - List of resolutions selected by the user
    /// * `source_codec` - Source video codec name (for passthrough detection)
    /// * `encryption` - Enable AES-128 encryption (uses TS segments), or disable (uses fMP4 segments)
    pub async fn transform_with_resolutions(
        &self,
        input_url: &str,
        input_height: Option<u32>,
        codec: Codec,
        selected_resolutions: &[HlsResolution],
        source_codec: Option<&str>,
        encryption: bool,
        progress: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
        duration: Option<f64>,
    ) -> Result<(TransformResult, TransformConfig), VideoError> {
        let transform_config =
            TransformConfig::for_resolutions(input_height, selected_resolutions, source_codec);

        // Validate we have at least 2 resolutions
        if transform_config.resolutions.len() < 2 {
            return Err(VideoError::InvalidInput(
                "At least 2 resolutions required for HLS adaptive streaming".to_string(),
            ));
        }

        info!(
            url = %input_url,
            resolutions = %transform_config.resolution_label(),
            hwaccel = %self.hwaccel,
            codec = %codec.as_str(),
            encryption = %encryption,
            "Starting HLS video transformation"
        );

        // Create temp directory
        let temp_dir = TempDir::new(&self.config.temp_dir).await?;
        let output_dir = temp_dir.path();

        debug!(path = %output_dir.display(), "Created temp directory");

        // Build FFmpeg command with hardware acceleration
        let mut ffmpeg = FfmpegCommand::new(
            input_url,
            output_dir,
            transform_config.clone(),
            self.hwaccel,
            codec,
        );

        if let Some(d) = duration {
            ffmpeg = ffmpeg.with_duration(d);
        }

        // Only enable encryption if requested (uses TS segments)
        // Without encryption, uses fMP4 segments (Safari compatible for HEVC)
        let encryption_key_base64 = if encryption {
            // Generate AES-128 encryption key
            let encryption_key = generate_aes_key();
            let encryption_key_base64 = key_to_base64(&encryption_key);

            // Write key file for FFmpeg
            let key_path = output_dir.join("encryption.key");
            fs::write(&key_path, &encryption_key).await?;

            // Write key info file for FFmpeg (format: key_uri\nkey_file_path)
            // Use placeholder URI - players must inject key from Nostr event
            let key_info_path = output_dir.join("key_info.txt");
            let key_info_content =
                format!("{}\n{}", ENCRYPTION_KEY_PLACEHOLDER_URI, key_path.display());
            fs::write(&key_info_path, key_info_content).await?;

            debug!(key_path = %key_path.display(), "Generated AES encryption key");

            ffmpeg = ffmpeg.with_encryption(&key_info_path);
            encryption_key_base64
        } else {
            debug!("Encryption disabled, using fMP4 segments");
            String::new()
        };

        ffmpeg.run(&self.config.ffmpeg_path, progress).await?;

        info!("FFmpeg HLS processing complete");

        // Collect output files
        let result = self
            .collect_output_files(temp_dir, encryption_key_base64)
            .await?;

        info!(
            master = %result.master_playlist_path.display(),
            streams = result.stream_playlists.len(),
            segments = result.segment_paths.len(),
            "HLS transformation complete"
        );

        Ok((result, transform_config))
    }

    /// Transform a video URL into a single MP4 file
    pub async fn transform_mp4(
        &self,
        input_url: &str,
        resolution: Resolution,
        quality: Option<u32>,
        codec: Codec,
        progress: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
        duration: Option<f64>,
    ) -> Result<Mp4TransformResult, VideoError> {
        info!(
            url = %input_url,
            resolution = %resolution.as_str(),
            hwaccel = %self.hwaccel,
            codec = %codec.as_str(),
            "Starting MP4 video transformation"
        );

        // Create temp directory
        let temp_dir = TempDir::new(&self.config.temp_dir).await?;
        let output_dir = temp_dir.path();

        debug!(path = %output_dir.display(), "Created temp directory");

        // Output file path
        let output_path = output_dir.join(format!("output_{}.mp4", resolution.as_str()));

        // Build and run FFmpeg command with hardware acceleration
        let mut ffmpeg = FfmpegMp4Command::new(
            input_url,
            output_path.clone(),
            resolution,
            self.hwaccel,
            codec,
        );
        if let Some(q) = quality {
            ffmpeg = ffmpeg.with_crf(q);
        }
        if let Some(d) = duration {
            ffmpeg = ffmpeg.with_duration(d);
        }
        ffmpeg.run(&self.config.ffmpeg_path, progress).await?;

        info!(output = %output_path.display(), "MP4 transformation complete");

        Ok(Mp4TransformResult {
            output_path,
            temp_dir,
        })
    }

    async fn collect_output_files(
        &self,
        temp_dir: TempDir,
        encryption_key: String,
    ) -> Result<TransformResult, VideoError> {
        let output_dir = temp_dir.path();
        let mut stream_playlists = Vec::new();
        let mut segment_paths = Vec::new();
        let mut stream_sizes = Vec::new();

        let mut entries = fs::read_dir(output_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            if name == "master.m3u8" {
                continue; // Handle separately
            } else if name.ends_with(".m3u8") {
                let metadata = entry.metadata().await?;
                stream_sizes.push(metadata.len());
                stream_playlists.push(path);
            } else if name.ends_with(".m4s")
                || name.ends_with(".ts")
                || (name.starts_with("init_") && name.ends_with(".mp4"))
            {
                segment_paths.push(path);
            }
        }

        // Sort for consistent ordering
        stream_playlists.sort();
        segment_paths.sort();

        let master_playlist_path = output_dir.join("master.m3u8");

        Ok(TransformResult {
            master_playlist_path,
            stream_playlists,
            segment_paths,
            stream_sizes,
            temp_dir,
            encryption_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_type() {
        assert_eq!(SegmentType::Fmp4.as_str(), "fmp4");
        assert_eq!(SegmentType::Fmp4.extension(), "m4s");
        assert_eq!(SegmentType::MpegTs.as_str(), "mpegts");
        assert_eq!(SegmentType::MpegTs.extension(), "ts");
    }

    #[test]
    fn test_default_transform_config() {
        let config = TransformConfig::default();
        assert_eq!(config.resolutions.len(), 5);
        assert!(config.resolutions.contains_key("240p"));
        assert!(config.resolutions.contains_key("360p"));
        assert!(config.resolutions.contains_key("480p"));
        assert!(config.resolutions.contains_key("720p"));
        assert!(config.resolutions.contains_key("1080p"));
        assert!(config.resolutions.get("1080p").unwrap().is_original);
        assert_eq!(config.hls_time, 6);
    }

    #[test]
    fn test_transform_config_for_4k_input() {
        let config = TransformConfig::for_resolution(Some(2160));
        assert_eq!(config.resolutions.len(), 6);
        assert!(config.resolutions.contains_key("240p"));
        assert!(config.resolutions.contains_key("360p"));
        assert!(config.resolutions.contains_key("480p"));
        assert!(config.resolutions.contains_key("720p"));
        assert!(config.resolutions.contains_key("1080p"));
        assert!(config.resolutions.contains_key("2160p"));

        // 1080p should be encoded (not original) for 4K input
        let r1080 = config.resolutions.get("1080p").unwrap();
        assert!(!r1080.is_original);
        // Width is auto-calculated to preserve aspect ratio
        assert_eq!(r1080.width, None);
        assert_eq!(r1080.height, Some(1080));

        // 2160p should be original
        assert!(config.resolutions.get("2160p").unwrap().is_original);
    }

    #[test]
    fn test_transform_config_for_non_4k_input() {
        let config = TransformConfig::for_resolution(Some(1080));
        assert_eq!(config.resolutions.len(), 5);
        assert!(config.resolutions.contains_key("240p"));
        assert!(config.resolutions.contains_key("360p"));
        assert!(config.resolutions.contains_key("480p"));
        assert!(config.resolutions.contains_key("720p"));
        assert!(config.resolutions.contains_key("1080p"));
        assert!(!config.resolutions.contains_key("2160p"));

        // 1080p should be original for non-4K input
        assert!(config.resolutions.get("1080p").unwrap().is_original);
    }

    #[test]
    fn test_resolution_label() {
        let config = TransformConfig::for_resolution(None);
        assert_eq!(config.resolution_label(), "240p, 360p, 480p, 720p, 1080p");

        let config_4k = TransformConfig::for_resolution(Some(2160));
        assert_eq!(
            config_4k.resolution_label(),
            "240p, 360p, 480p, 720p, 1080p, 2160p"
        );
    }

    #[test]
    fn test_for_resolutions_selected_subset() {
        let selected = vec![
            HlsResolution::R360p,
            HlsResolution::R720p,
            HlsResolution::Original,
        ];
        let config = TransformConfig::for_resolutions(Some(1080), &selected, Some("h264"));

        assert_eq!(config.resolutions.len(), 3);
        assert!(config.resolutions.contains_key("360p"));
        assert!(config.resolutions.contains_key("720p"));
        assert!(config.resolutions.contains_key("1080p")); // Original becomes 1080p

        // 1080p should be original (passthrough) since h264 is compatible
        assert!(config.resolutions.get("1080p").unwrap().is_original);
    }

    #[test]
    fn test_for_resolutions_incompatible_codec() {
        let selected = vec![
            HlsResolution::R360p,
            HlsResolution::R720p,
            HlsResolution::Original,
        ];
        let config = TransformConfig::for_resolutions(Some(1080), &selected, Some("vp9"));

        // 1080p should NOT be original (needs re-encode) since vp9 is not HLS-compatible
        let r1080 = config.resolutions.get("1080p").unwrap();
        assert!(!r1080.is_original);
        assert!(r1080.height.is_some()); // Has height for re-encoding (width auto-calculated)
    }

    #[test]
    fn test_for_resolutions_skips_higher_than_input() {
        let selected = vec![
            HlsResolution::R240p,
            HlsResolution::R360p,
            HlsResolution::R720p,
            HlsResolution::R1080p,
            HlsResolution::Original,
        ];
        let config = TransformConfig::for_resolutions(Some(480), &selected, None);

        // Only 240p, 360p, and original (at 480p level) should be included
        assert!(config.resolutions.contains_key("240p"));
        assert!(config.resolutions.contains_key("360p"));
        assert!(!config.resolutions.contains_key("720p")); // Skipped: higher than input
        assert!(!config.resolutions.contains_key("2160p"));
    }

    #[test]
    fn test_is_hls_compatible_codec() {
        assert!(TransformConfig::is_hls_compatible_codec("h264"));
        assert!(TransformConfig::is_hls_compatible_codec("H264"));
        assert!(TransformConfig::is_hls_compatible_codec("avc"));
        assert!(TransformConfig::is_hls_compatible_codec("avc1"));
        assert!(TransformConfig::is_hls_compatible_codec("h265"));
        assert!(TransformConfig::is_hls_compatible_codec("hevc"));
        assert!(TransformConfig::is_hls_compatible_codec("hvc1"));

        assert!(!TransformConfig::is_hls_compatible_codec("vp9"));
        assert!(!TransformConfig::is_hls_compatible_codec("av1"));
        assert!(!TransformConfig::is_hls_compatible_codec("mpeg4"));
    }
}
