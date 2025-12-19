use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;
use tracing::debug;

use crate::error::VideoError;

#[derive(Debug, Clone, Deserialize)]
pub struct VideoMetadata {
    pub format: FormatInfo,
    pub streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormatInfo {
    pub filename: String,
    pub duration: Option<String>,
    pub size: Option<String>,
    pub bit_rate: Option<String>,
    pub format_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamInfo {
    pub codec_name: Option<String>,
    pub codec_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bit_rate: Option<String>,
    #[serde(rename = "r_frame_rate")]
    pub frame_rate: Option<String>,
    pub channels: Option<u32>,
    pub sample_rate: Option<String>,
}

impl VideoMetadata {
    /// Extract metadata from a video file or URL using ffprobe
    pub async fn extract(input: &str, ffprobe_path: &Path) -> Result<Self, VideoError> {
        let output = Command::new(ffprobe_path)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                input,
            ])
            .output()
            .await
            .map_err(VideoError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VideoError::FfprobeFailed(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!(output = %stdout, "ffprobe output");

        let metadata: VideoMetadata =
            serde_json::from_str(&stdout).map_err(|e| VideoError::FfprobeFailed(e.to_string()))?;

        Ok(metadata)
    }

    /// Get the video stream info
    pub fn video_stream(&self) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.codec_type == "video")
    }

    /// Get the audio stream info
    pub fn audio_stream(&self) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.codec_type == "audio")
    }

    /// Get video duration in seconds
    pub fn duration_secs(&self) -> Option<f64> {
        self.format.duration.as_ref()?.parse().ok()
    }

    /// Get video resolution as (width, height)
    pub fn resolution(&self) -> Option<(u32, u32)> {
        let video = self.video_stream()?;
        Some((video.width?, video.height?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metadata() {
        let json = r#"{
            "format": {
                "filename": "test.mp4",
                "duration": "120.5",
                "size": "1024000",
                "bit_rate": "1000000",
                "format_name": "mov,mp4,m4a,3gp,3g2,mj2"
            },
            "streams": [
                {
                    "codec_name": "h264",
                    "codec_type": "video",
                    "width": 1920,
                    "height": 1080,
                    "bit_rate": "800000",
                    "r_frame_rate": "30/1"
                },
                {
                    "codec_name": "aac",
                    "codec_type": "audio",
                    "channels": 2,
                    "sample_rate": "48000"
                }
            ]
        }"#;

        let metadata: VideoMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.format.filename, "test.mp4");
        assert_eq!(metadata.duration_secs(), Some(120.5));
        assert_eq!(metadata.resolution(), Some((1920, 1080)));

        let video = metadata.video_stream().unwrap();
        assert_eq!(video.codec_name.as_deref(), Some("h264"));

        let audio = metadata.audio_stream().unwrap();
        assert_eq!(audio.channels, Some(2));
    }
}
