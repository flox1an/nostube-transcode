use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::process::Command as TokioCommand;
use tracing::debug;

use crate::dvm::events::{Codec, Resolution};
use crate::error::VideoError;
use crate::video::hwaccel::HwAccel;
use crate::video::transform::TransformConfig;

pub use self::FfmpegMp4Command as Mp4Command;

pub struct FfmpegCommand {
    input: String,
    output_dir: std::path::PathBuf,
    config: TransformConfig,
    hwaccel: HwAccel,
    codec: Codec,
    /// Path to HLS key info file for AES-128 encryption
    key_info_path: Option<PathBuf>,
}

impl FfmpegCommand {
    pub fn new(
        input: &str,
        output_dir: &Path,
        config: TransformConfig,
        hwaccel: HwAccel,
        codec: Codec,
    ) -> Self {
        Self {
            input: input.to_string(),
            output_dir: output_dir.to_path_buf(),
            config,
            hwaccel,
            codec,
            key_info_path: None,
        }
    }

    /// Enable AES-128 encryption with the given key info file
    pub fn with_encryption(mut self, key_info_path: &Path) -> Self {
        self.key_info_path = Some(key_info_path.to_path_buf());
        self
    }

    /// Build the FFmpeg command
    pub fn build(&self) -> Command {
        let mut cmd = Command::new("ffmpeg");

        // Input
        cmd.arg("-i").arg(&self.input);

        // Build complex filter for scaling
        let filter = self.build_complex_filter();
        if !filter.is_empty() {
            cmd.arg("-filter_complex").arg(&filter);
        }

        // Add mappings and codec settings
        self.add_output_options(&mut cmd);

        // HLS options
        cmd.arg("-f")
            .arg("hls")
            .arg("-var_stream_map")
            .arg(self.build_var_stream_map())
            .arg("-hls_time")
            .arg(self.config.hls_time.to_string())
            .arg("-hls_list_size")
            .arg(self.config.hls_list_size.to_string())
            .arg("-hls_segment_type")
            .arg(self.config.segment_type.as_str())
            .arg("-master_pl_name")
            .arg("master.m3u8")
            .arg("-hls_segment_filename")
            .arg(self.output_dir.join(format!(
                "stream_%v_%03d.{}",
                self.config.segment_type.extension()
            )));

        // Output pattern
        let output = self.output_dir.join("stream_%v.m3u8");
        cmd.arg(output);

        cmd
    }

    /// Run the FFmpeg command asynchronously
    pub async fn run(&self, ffmpeg_path: &Path) -> Result<(), VideoError> {
        let mut cmd = TokioCommand::new(ffmpeg_path);

        // Overwrite without asking
        cmd.arg("-y");

        // Hardware acceleration input options (before -i)
        self.add_hwaccel_input_options(&mut cmd);

        // Input
        cmd.arg("-i").arg(&self.input);

        // Build complex filter for scaling
        let filter = self.build_complex_filter();
        if !filter.is_empty() {
            cmd.arg("-filter_complex").arg(&filter);
        }

        // Add mappings and codec settings
        self.add_output_options_tokio(&mut cmd);

        // HLS options
        // Note: When encryption is used, segment_type must be mpegts (FFmpeg limitation)
        let segment_type = if self.key_info_path.is_some() {
            "mpegts"
        } else {
            self.config.segment_type.as_str()
        };
        let segment_ext = if self.key_info_path.is_some() {
            "ts"
        } else {
            self.config.segment_type.extension()
        };

        cmd.arg("-f")
            .arg("hls")
            .arg("-var_stream_map")
            .arg(self.build_var_stream_map())
            .arg("-hls_time")
            .arg(self.config.hls_time.to_string())
            .arg("-hls_list_size")
            .arg(self.config.hls_list_size.to_string())
            .arg("-hls_segment_type")
            .arg(segment_type)
            .arg("-master_pl_name")
            .arg("master.m3u8")
            .arg("-hls_segment_filename")
            .arg(
                self.output_dir
                    .join(format!("stream_%v_%03d.{}", segment_ext)),
            );

        // Add AES-128 encryption if key info file is provided
        if let Some(ref key_info_path) = self.key_info_path {
            cmd.arg("-hls_key_info_file").arg(key_info_path);
        }

        // Output pattern
        let output = self.output_dir.join("stream_%v.m3u8");
        cmd.arg(output);

        debug!(command = ?cmd, hwaccel = %self.hwaccel, "Running FFmpeg");

        // In debug mode, show FFmpeg output in real-time
        if tracing::enabled!(tracing::Level::DEBUG) {
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::inherit());

            let status = cmd.status().await.map_err(VideoError::Io)?;

            if !status.success() {
                return Err(VideoError::FfmpegFailed(
                    "FFmpeg failed (see output above)".to_string(),
                ));
            }
        } else {
            let output = cmd.output().await.map_err(VideoError::Io)?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(VideoError::FfmpegFailed(stderr.to_string()));
            }
        }

        Ok(())
    }

    /// Add hardware acceleration input options
    fn add_hwaccel_input_options(&self, cmd: &mut TokioCommand) {
        // Initialize hardware device for filter graphs
        if let Some(init_device) = self.hwaccel.init_hw_device() {
            cmd.arg("-init_hw_device").arg(&init_device);
        }

        // Tell FFmpeg which device to use for filter operations (needed for hwupload)
        if let Some(filter_device) = self.hwaccel.filter_hw_device() {
            cmd.arg("-filter_hw_device").arg(filter_device);
        }

        // Hardware accelerated decoding
        if let Some(hwaccel_type) = self.hwaccel.hwaccel_type() {
            cmd.arg("-hwaccel").arg(hwaccel_type);

            // QSV-specific device
            if let Some(device) = self.hwaccel.qsv_device() {
                cmd.arg("-qsv_device").arg(device);
            }

            // Keep frames in hardware memory
            if let Some(output_format) = self.hwaccel.hwaccel_output_format() {
                cmd.arg("-hwaccel_output_format").arg(output_format);
            }
        }

        // Enable multi-threaded decoding for software decoders (e.g., libdav1d for AV1)
        // This significantly improves decode performance for CPU-decoded formats
        cmd.arg("-threads").arg("0");
    }

    fn build_complex_filter(&self) -> String {
        let non_original: Vec<_> = self
            .config
            .resolutions
            .iter()
            .filter(|(_, r)| !r.is_original)
            .collect();

        if non_original.is_empty() {
            // No scaling needed - all resolutions are original, no filter graph
            return String::new();
        }

        let mut parts = Vec::new();
        let scale_filter = self.hwaccel.scale_filter();

        // Split filter - only for non-original resolutions
        // Original stream will be mapped directly from input (0:v) to allow stream copy
        let mut output_labels: Vec<String> = Vec::new();

        for (name, _) in &non_original {
            output_labels.push(format!("[{}]", name));
        }

        // Build the initial filter chain
        // For hardware acceleration that needs explicit frame upload (e.g., QSV when hwaccel_output_format
        // is not set), prepend the hwupload filter to convert software frames to hardware frames.
        // This handles cases where hardware decoding falls back to software (e.g., QSV can't decode AV1).
        let input_chain = if self.hwaccel.hwaccel_output_format().is_none() {
            if let Some(upload_filter) = self.hwaccel.upload_filter() {
                // Upload frames to hardware memory before splitting/scaling
                // The upload_filter already includes format conversion (e.g., format=nv12 for QSV)
                format!(
                    "[0:v]{},split={}{}",
                    upload_filter,
                    non_original.len(),
                    output_labels.join("")
                )
            } else {
                format!(
                    "[0:v]split={}{}",
                    non_original.len(),
                    output_labels.join("")
                )
            }
        } else {
            // hwaccel_output_format is set, so frames are already in hardware memory
            format!(
                "[0:v]split={}{}",
                non_original.len(),
                output_labels.join("")
            )
        };
        parts.push(input_chain);

        // Scale filters for non-original resolutions using appropriate hardware filter
        // Use -2 for width to auto-calculate while preserving aspect ratio (and ensuring even dimensions)
        for (name, res) in &non_original {
            match (res.width, res.height) {
                (Some(w), Some(h)) => {
                    // Both dimensions specified
                    parts.push(format!(
                        "[{}]{}=w={}:h={}[{}out]",
                        name, scale_filter, w, h, name
                    ));
                }
                (None, Some(h)) => {
                    // Only height specified - auto-calculate width to preserve aspect ratio
                    parts.push(format!(
                        "[{}]{}=w=-2:h={}[{}out]",
                        name, scale_filter, h, name
                    ));
                }
                (Some(w), None) => {
                    // Only width specified - auto-calculate height to preserve aspect ratio
                    parts.push(format!(
                        "[{}]{}=w={}:h=-2[{}out]",
                        name, scale_filter, w, name
                    ));
                }
                (None, None) => {
                    // No dimensions - should not happen for non-original, skip
                }
            }
        }

        // Note: Original stream is NOT included in filter graph
        // It will be mapped directly from 0:v to allow -c:v copy

        parts.join(";")
    }

    fn build_var_stream_map(&self) -> String {
        (0..self.config.resolutions.len())
            .map(|i| format!("v:{},a:{}", i, i))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn add_output_options(&self, cmd: &mut Command) {
        let mut keys: Vec<_> = self.config.resolutions.keys().collect();
        keys.sort(); // Consistent ordering

        for (idx, key) in keys.iter().enumerate() {
            let res = &self.config.resolutions[*key];

            if res.is_original {
                // Map directly from input stream to allow stream copy
                // (cannot use copy with filter graph outputs)
                cmd.arg("-map")
                    .arg("0:v")
                    .arg(format!("-c:v:{}", idx))
                    .arg("copy");
            } else {
                let codec = res.video_codec.as_deref().unwrap_or("libx265");
                cmd.arg("-map")
                    .arg(format!("[{}out]", key))
                    .arg(format!("-c:v:{}", idx))
                    .arg(codec);

                // Add hvc1 tag for Safari/iOS compatibility when using H.265
                if codec == "libx265" || codec.contains("hevc") {
                    cmd.arg(format!("-tag:v:{}", idx)).arg("hvc1");
                }

                if let Some(q) = res.quality {
                    cmd.arg(format!("-crf:{}", idx)).arg(q.to_string());
                }

                if let Some(br) = &res.video_bitrate {
                    cmd.arg(format!("-b:v:{}", idx)).arg(br);
                }
            }

            // Audio
            cmd.arg("-map")
                .arg("0:a")
                .arg(format!("-c:a:{}", idx))
                .arg(res.audio_codec.as_deref().unwrap_or("aac"));

            if let Some(br) = &res.audio_bitrate {
                cmd.arg(format!("-b:a:{}", idx)).arg(br);
            }
        }
    }

    fn add_output_options_tokio(&self, cmd: &mut TokioCommand) {
        let mut keys: Vec<_> = self.config.resolutions.keys().collect();
        keys.sort(); // Consistent ordering

        let encoder = self.hwaccel.video_encoder(self.codec);

        for (idx, key) in keys.iter().enumerate() {
            let res = &self.config.resolutions[*key];

            if res.is_original {
                // Map directly from input stream to allow stream copy
                // (cannot use copy with filter graph outputs)
                cmd.arg("-map")
                    .arg("0:v")
                    .arg(format!("-c:v:{}", idx))
                    .arg("copy");
            } else {
                // Use hardware encoder if available, or override from config
                let video_codec = res.video_codec.as_deref().unwrap_or(encoder);
                cmd.arg("-map")
                    .arg(format!("[{}out]", key))
                    .arg(format!("-c:v:{}", idx))
                    .arg(video_codec);

                // Add hvc1 tag for Safari/iOS compatibility when using H.265
                if self.codec == Codec::H265
                    || video_codec.contains("hevc")
                    || video_codec.contains("265")
                {
                    cmd.arg(format!("-tag:v:{}", idx)).arg("hvc1");
                }

                // Add quality parameter based on hardware acceleration type
                if let Some(q) = res.quality {
                    let (quality_param, quality_value) = self.hwaccel.quality_param(q);
                    // For per-stream quality, append stream index
                    let param_with_idx =
                        format!("{}:{}", quality_param.trim_start_matches('-'), idx);
                    cmd.arg(format!("-{}", param_with_idx)).arg(&quality_value);
                }

                // Add encoder-specific options (only for first encoded stream to avoid duplicates)
                if idx == 0
                    || !keys
                        .iter()
                        .take(idx)
                        .any(|k| !self.config.resolutions[*k].is_original)
                {
                    for (opt, val) in self.hwaccel.encoder_options(self.codec) {
                        cmd.arg(opt).arg(val);
                    }
                }

                if let Some(br) = &res.video_bitrate {
                    cmd.arg(format!("-b:v:{}", idx)).arg(br);
                }
            }

            // Audio
            cmd.arg("-map")
                .arg("0:a")
                .arg(format!("-c:a:{}", idx))
                .arg(res.audio_codec.as_deref().unwrap_or("aac"));

            if let Some(br) = &res.audio_bitrate {
                cmd.arg(format!("-b:a:{}", idx)).arg(br);
            }
        }
    }
}

/// FFmpeg command builder for single MP4 output
pub struct FfmpegMp4Command {
    input: String,
    output_path: PathBuf,
    resolution: Resolution,
    crf: u32,
    audio_bitrate: String,
    hwaccel: HwAccel,
    codec: Codec,
}

impl FfmpegMp4Command {
    pub fn new(
        input: &str,
        output_path: PathBuf,
        resolution: Resolution,
        hwaccel: HwAccel,
        codec: Codec,
    ) -> Self {
        Self {
            input: input.to_string(),
            output_path,
            resolution,
            crf: 23,
            audio_bitrate: "128k".to_string(),
            hwaccel,
            codec,
        }
    }

    /// Set the CRF (quality) value
    pub fn with_crf(mut self, crf: u32) -> Self {
        self.crf = crf;
        self
    }

    /// Run the FFmpeg MP4 encoding command asynchronously
    pub async fn run(&self, ffmpeg_path: &Path) -> Result<(), VideoError> {
        let mut cmd = TokioCommand::new(ffmpeg_path);

        // Overwrite without asking
        cmd.arg("-y");

        // Hardware acceleration input options (before -i)
        self.add_hwaccel_input_options(&mut cmd);

        // Input
        cmd.arg("-i").arg(&self.input);

        // Scale filter using appropriate hardware filter
        // Use -2 for width to auto-calculate while preserving aspect ratio (and ensuring even dimensions)
        let (_width, height) = self.resolution.dimensions();
        let scale_filter = self.hwaccel.scale_filter();

        // For QSV, when hwaccel_output_format is not set (to handle software decode fallback),
        // we need to upload frames to QSV memory before applying QSV filters
        // The upload_filter already includes format conversion (e.g., format=nv12 for QSV)
        let vf = if self.hwaccel.hwaccel_output_format().is_none() {
            if let Some(upload_filter) = self.hwaccel.upload_filter() {
                format!("{},{}=w=-2:h={}", upload_filter, scale_filter, height)
            } else {
                format!("{}=w=-2:h={}", scale_filter, height)
            }
        } else {
            format!("{}=w=-2:h={}", scale_filter, height)
        };
        cmd.arg("-vf").arg(vf);

        // Video codec with hardware acceleration
        let encoder = self.hwaccel.video_encoder(self.codec);
        cmd.arg("-c:v").arg(encoder);

        // Add hvc1 tag for Safari/iOS compatibility (H.265 only)
        if self.codec == Codec::H265 {
            cmd.arg("-tag:v").arg("hvc1");
        }

        // Quality parameter
        let (quality_param, quality_value) = self.hwaccel.quality_param(self.crf);
        cmd.arg(quality_param).arg(&quality_value);

        // Encoder-specific options
        for (opt, val) in self.hwaccel.encoder_options(self.codec) {
            cmd.arg(opt).arg(val);
        }

        // Audio codec
        cmd.arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg(&self.audio_bitrate);

        // Output file
        cmd.arg(&self.output_path);

        debug!(command = ?cmd, hwaccel = %self.hwaccel, "Running FFmpeg MP4 encoding");

        // In debug mode, show FFmpeg output in real-time
        if tracing::enabled!(tracing::Level::DEBUG) {
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::inherit());

            let status = cmd.status().await.map_err(VideoError::Io)?;

            if !status.success() {
                return Err(VideoError::FfmpegFailed(
                    "FFmpeg failed (see output above)".to_string(),
                ));
            }
        } else {
            let output = cmd.output().await.map_err(VideoError::Io)?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(VideoError::FfmpegFailed(stderr.to_string()));
            }
        }

        Ok(())
    }

    /// Add hardware acceleration input options
    fn add_hwaccel_input_options(&self, cmd: &mut TokioCommand) {
        // Initialize hardware device
        if let Some(init_device) = self.hwaccel.init_hw_device() {
            cmd.arg("-init_hw_device").arg(&init_device);
        }

        // Tell FFmpeg which device to use for filter operations (needed for hwupload)
        if let Some(filter_device) = self.hwaccel.filter_hw_device() {
            cmd.arg("-filter_hw_device").arg(filter_device);
        }

        // Hardware accelerated decoding
        if let Some(hwaccel_type) = self.hwaccel.hwaccel_type() {
            cmd.arg("-hwaccel").arg(hwaccel_type);

            // QSV-specific device
            if let Some(device) = self.hwaccel.qsv_device() {
                cmd.arg("-qsv_device").arg(device);
            }

            // Keep frames in hardware memory
            if let Some(output_format) = self.hwaccel.hwaccel_output_format() {
                cmd.arg("-hwaccel_output_format").arg(output_format);
            }
        }

        // Enable multi-threaded decoding for software decoders (e.g., libdav1d for AV1)
        // This significantly improves decode performance for CPU-decoded formats
        cmd.arg("-threads").arg("0");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn test_ffmpeg_command_building() {
        let config = TransformConfig::default();
        let cmd = FfmpegCommand::new(
            "input.mp4",
            Path::new("/tmp/output"),
            config,
            HwAccel::Software,
            Codec::H264,
        );

        let built = cmd.build();
        let args: Vec<&OsStr> = built.get_args().collect();

        // Check essential arguments
        assert!(args.contains(&OsStr::new("-f")));
        assert!(args.contains(&OsStr::new("hls")));
        assert!(args.contains(&OsStr::new("-i")));
        assert!(args.contains(&OsStr::new("input.mp4")));
    }

    #[test]
    fn test_hwaccel_detection() {
        // Just verify detection doesn't panic
        let hwaccel = HwAccel::detect();
        assert!(!hwaccel.video_encoder(Codec::H264).is_empty());
        assert!(!hwaccel.video_encoder(Codec::H265).is_empty());
    }
}
