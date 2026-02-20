use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use tracing::debug;
use tracing::info;

use crate::dvm::events::Codec;

/// Hardware acceleration backend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HwAccel {
    /// NVIDIA NVENC (Linux/Windows)
    Nvenc,
    /// Intel/AMD VAAPI (Linux) - works on all Intel generations
    Vaapi,
    /// Intel Quick Sync Video (Linux) - legacy, requires libmfx
    Qsv,
    /// Apple VideoToolbox (macOS)
    VideoToolbox,
    /// Software encoding (fallback)
    #[default]
    Software,
}

impl HwAccel {
    /// Detect the best available hardware acceleration
    #[allow(unreachable_code)]
    pub fn detect() -> Self {
        // macOS: use VideoToolbox
        #[cfg(target_os = "macos")]
        {
            info!("Detected macOS, using VideoToolbox hardware acceleration");
            return Self::VideoToolbox;
        }

        // Linux: check for NVIDIA first (usually faster), then VAAPI (works on all Intel), then QSV
        #[cfg(target_os = "linux")]
        {
            if Self::is_nvidia_available() {
                info!("Detected NVIDIA GPU, using NVENC hardware acceleration");
                return Self::Nvenc;
            }

            if Self::is_vaapi_available() {
                info!("Detected VAAPI hardware acceleration");
                return Self::Vaapi;
            }

            if Self::is_qsv_available() {
                info!("Detected Intel QSV hardware acceleration");
                return Self::Qsv;
            }
        }

        info!("No hardware acceleration detected, using software encoding");
        Self::Software
    }

    /// Detect all available hardware acceleration methods
    #[allow(unreachable_code)]
    pub fn detect_all() -> Vec<Self> {
        let mut available = Vec::new();

        #[cfg(target_os = "macos")]
        {
            available.push(Self::VideoToolbox);
        }

        #[cfg(target_os = "linux")]
        {
            if Self::is_nvidia_available() {
                available.push(Self::Nvenc);
            }
            if Self::is_vaapi_available() {
                available.push(Self::Vaapi);
            }
            if Self::is_qsv_available() {
                available.push(Self::Qsv);
            }
        }

        // Software is always available
        available.push(Self::Software);
        available
    }

    /// Get the name of this hardware acceleration method
    pub fn name(&self) -> &'static str {
        match self {
            Self::Nvenc => "NVIDIA NVENC",
            Self::Vaapi => "VAAPI",
            Self::Qsv => "Intel QSV",
            Self::VideoToolbox => "Apple VideoToolbox",
            Self::Software => "Software",
        }
    }

    /// Check if NVIDIA GPU is available (Linux)
    #[cfg(target_os = "linux")]
    fn is_nvidia_available() -> bool {
        // Check for NVIDIA device
        let nvidia_devices = ["/dev/nvidia0", "/dev/nvidiactl"];

        for device in &nvidia_devices {
            if Path::new(device).exists() {
                info!(device = %device, "Found NVIDIA device");
                return true;
            }
        }

        false
    }

    /// Check if VAAPI is available (Linux)
    /// This runs a quick FFmpeg probe to verify VAAPI HEVC encoding and AV1 decoding capabilities.
    #[cfg(target_os = "linux")]
    fn is_vaapi_available() -> bool {
        // First check for render device
        let render_devices = ["/dev/dri/renderD128", "/dev/dri/renderD129"];

        let device = render_devices.iter().find(|d| Path::new(*d).exists());

        let Some(device) = device else {
            debug!("No render device found, VAAPI unavailable");
            return false;
        };

        debug!(device = %device, "Found render device, testing VAAPI capabilities");

        // --- Test HEVC VAAPI encoding ---
        // This probe verifies that FFmpeg can use the VAAPI device for HEVC encoding.
        let hevc_result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                &format!("vaapi=vaapi:{}", device),
                "-filter_hw_device",
                "vaapi",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=64x64:d=0.1", // Dummy input source
                "-vf",
                "format=nv12,hwupload", // Ensure frames are in VAAPI memory
                "-c:v",
                "hevc_vaapi",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        let mut hevc_ok = false;
        match hevc_result {
            Ok(output) if output.status.success() => {
                info!(device = %device, "VAAPI HEVC encoding verified");
                hevc_ok = true;
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    device = %device,
                    stderr = %stderr,
                    "VAAPI HEVC probe failed"
                );
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg VAAPI HEVC probe");
            }
        }

        // --- Test AV1 VAAPI decoding ---
        // This probe verifies that FFmpeg can use the VAAPI device for AV1 decoding.
        // We use a dummy input and try to decode it using av1_vaapi.
        let av1_result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                &format!("vaapi=vaapi:{}", device),
                "-filter_hw_device",
                "vaapi",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=0.1", // Dummy input source
                "-vf",
                "format=nv12,hwupload", // Ensure frames are in VAAPI memory
                "-c:v",
                "av1_vaapi", // Explicitly request AV1 VAAPI decoder
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        let mut av1_ok = false;
        match av1_result {
            Ok(output) if output.status.success() => {
                info!(device = %device, "VAAPI AV1 decoding verified");
                av1_ok = true;
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    device = %device,
                    stderr = %stderr,
                    "VAAPI AV1 probe failed"
                );
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg VAAPI AV1 probe");
            }
        }

        // VAAPI is considered available if either HEVC encoding OR AV1 decoding works.
        // For N100, AV1 decoding is critical.
        hevc_ok || av1_ok
    }

    /// Check if Intel QSV is available (Linux)
    /// This runs a quick FFmpeg probe to verify QSV actually works,
    /// not just that the render device exists (which could be AMD or unsupported Intel).
    #[cfg(target_os = "linux")]
    fn is_qsv_available() -> bool {
        // First check for render device
        let render_devices = ["/dev/dri/renderD128", "/dev/dri/renderD129"];

        let device = render_devices.iter().find(|d| Path::new(*d).exists());

        let Some(device) = device else {
            debug!("No render device found, QSV unavailable");
            return false;
        };

        debug!(device = %device, "Found render device, testing QSV initialization");

        // Run a quick FFmpeg test to verify QSV actually works
        // This catches cases where the device exists but:
        // - It's an AMD GPU (not Intel)
        // - Intel GPU doesn't support QSV
        // - MFX/oneVPL runtime is not installed
        let result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                &format!("qsv=qsv:hw_any,child_device={}", device),
                "-filter_hw_device",
                "qsv",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=64x64:d=0.1",
                "-vf",
                "format=nv12,hwupload=extra_hw_frames=64,scale_qsv=64:64",
                "-c:v",
                "hevc_qsv",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!(device = %device, "QSV hardware acceleration verified");
                true
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    device = %device,
                    stderr = %stderr,
                    "QSV probe failed, falling back to software encoding"
                );
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg QSV probe");
                false
            }
        }
    }

    /// Get the QSV device path (if available)
    pub fn qsv_device(&self) -> Option<&'static str> {
        match self {
            Self::Qsv => {
                // Return the first available device
                for device in &["/dev/dri/renderD128", "/dev/dri/renderD129"] {
                    if Path::new(device).exists() {
                        return Some(device);
                    }
                }
                Some("/dev/dri/renderD128") // fallback
            }
            _ => None,
        }
    }

    /// Get the VAAPI device path (if available)
    pub fn vaapi_device(&self) -> Option<&'static str> {
        match self {
            Self::Vaapi => {
                // Return the first available device
                for device in &["/dev/dri/renderD128", "/dev/dri/renderD129"] {
                    if Path::new(device).exists() {
                        return Some(device);
                    }
                }
                Some("/dev/dri/renderD128") // fallback
            }
            _ => None,
        }
    }

    /// Get the video encoder name for this acceleration and codec
    pub fn video_encoder(&self, codec: Codec) -> &'static str {
        match (self, codec) {
            (Self::Nvenc, Codec::H264) => "h264_nvenc",
            (Self::Nvenc, Codec::H265) => "hevc_nvenc",
            (Self::Vaapi, Codec::H264) => "h264_vaapi",
            (Self::Vaapi, Codec::H265) => "hevc_vaapi",
            (Self::Qsv, Codec::H264) => "h264_qsv",
            (Self::Qsv, Codec::H265) => "hevc_qsv",
            (Self::VideoToolbox, Codec::H264) => "h264_videotoolbox",
            (Self::VideoToolbox, Codec::H265) => "hevc_videotoolbox",
            (Self::Software, Codec::H264) => "libx264",
            (Self::Software, Codec::H265) => "libx265",
        }
    }

    /// Get the scale filter name for this acceleration
    pub fn scale_filter(&self) -> &'static str {
        match self {
            Self::Nvenc => "scale_cuda",
            Self::Vaapi => "scale_vaapi",
            Self::Qsv => "scale_qsv",
            Self::VideoToolbox => "scale",
            Self::Software => "scale",
        }
    }

    /// Whether this uses hardware-accelerated decoding
    pub fn uses_hw_decode(&self) -> bool {
        matches!(self, Self::Nvenc | Self::Vaapi | Self::Qsv)
    }

    /// Get hwaccel type for FFmpeg -hwaccel option
    pub fn hwaccel_type(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("cuda"),
            Self::Vaapi => Some("vaapi"),
            Self::Qsv => Some("qsv"),
            _ => None,
        }
    }

    /// Get hwaccel output format for FFmpeg -hwaccel_output_format option
    /// This keeps decoded frames in GPU memory for efficient hardware encoding.
    pub fn hwaccel_output_format(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("cuda"),
            // VAAPI: Keep frames in GPU memory. If hardware decoding falls back to software,
            // the filter graph will handle the upload via upload_filter().
            Self::Vaapi => Some("vaapi"),
            // QSV: Don't use hwaccel_output_format because QSV can't decode all codecs
            // (e.g., AV1 on many platforms). When HW decode fails, FFmpeg falls back to
            // software decoding which outputs software frames. If hwaccel_output_format=qsv
            // is set, FFmpeg incorrectly assumes frames are in QSV memory, causing
            // "Impossible to convert between formats" errors with QSV filters.
            // Instead, we use upload_filter() to explicitly upload frames to QSV memory.
            Self::Qsv => None,
            _ => None,
        }
    }

    /// Get quality parameter name and value
    /// Returns (param_name, value) for the given CRF-equivalent quality
    pub fn quality_param(&self, crf: u32) -> (&'static str, String) {
        match self {
            Self::Nvenc => {
                // NVENC uses -cq for constant quality (similar to CRF)
                ("-cq", crf.to_string())
            }
            Self::Vaapi => {
                // VAAPI uses -qp for constant QP mode (similar scale to CRF, lower = better)
                ("-qp", crf.to_string())
            }
            Self::Qsv => {
                // QSV uses global_quality (similar scale to CRF, lower = better)
                ("-global_quality", crf.to_string())
            }
            Self::VideoToolbox => {
                // VideoToolbox uses q:v (0-100, higher = better quality)
                // Map CRF 18-28 to q:v 75-55 roughly
                let q = 100 - (crf * 2).min(80);
                ("-q:v", q.to_string())
            }
            Self::Software => ("-crf", crf.to_string()),
        }
    }

    /// Get additional encoder options
    pub fn encoder_options(&self, codec: Codec) -> Vec<(&'static str, &'static str)> {
        match (self, codec) {
            (Self::Nvenc, _) => vec![
                ("-preset", "p4"), // balanced preset
                ("-tune", "hq"),
                ("-rc", "vbr"),
                ("-g", "60"),
                ("-keyint_min", "60"),
            ],
            (Self::Vaapi, _) => vec![
                // VAAPI doesn't have many options, but we set profile for compatibility
                ("-profile:v", "main"),
                // GOP size for HLS segment alignment (approx 2s at 30-60fps)
                ("-g", "60"),
                ("-keyint_min", "60"),
            ],
            (Self::Qsv, _) => vec![
                ("-preset", "medium"),
                ("-g", "60"),
                ("-keyint_min", "60"),
            ],
            (Self::VideoToolbox, _) => vec![],
            (Self::Software, _) => vec![("-preset", "medium")],
        }
    }

    /// Get init_hw_device option for complex filter graphs
    pub fn init_hw_device(&self) -> Option<String> {
        match self {
            Self::Nvenc => Some("cuda=cuda:0".to_string()),
            Self::Vaapi => {
                let device = self.vaapi_device().unwrap_or("/dev/dri/renderD128");
                Some(format!("vaapi=vaapi:{}", device))
            }
            Self::Qsv => {
                let device = self.qsv_device().unwrap_or("/dev/dri/renderD128");
                Some(format!("qsv=qsv:hw_any,child_device={}", device))
            }
            _ => None,
        }
    }

    /// Get filter_hw_device option name (used with hwupload in filter graphs)
    /// This tells FFmpeg which initialized device to use for filter operations
    pub fn filter_hw_device(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("cuda"),
            Self::Vaapi => Some("vaapi"),
            Self::Qsv => Some("qsv"),
            _ => None,
        }
    }

    /// Get hardware upload filter for transitioning from software to hardware frames
    pub fn upload_filter(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("hwupload_cuda"),
            // VAAPI: Convert to nv12 (required by VAAPI encoders) and upload to VAAPI memory.
            // The format filter handles pixel format conversion including 10-bit to 8-bit
            // (e.g., AV1 decoded by libdav1d outputs yuv420p10le, converted to nv12).
            // extra_hw_frames=64 provides buffer for frame reordering during encoding.
            Self::Vaapi => Some("format=nv12,hwupload=extra_hw_frames=64"),
            // QSV: Convert to nv12 (required by QSV) and upload to QSV memory.
            // The format filter handles pixel format conversion including 10-bit to 8-bit
            // (e.g., AV1 decoded by libdav1d outputs yuv420p10le, converted to nv12).
            // extra_hw_frames=64 provides buffer for frame reordering during encoding.
            Self::Qsv => Some("format=nv12,hwupload=extra_hw_frames=64"),
            _ => None,
        }
    }
}

impl std::fmt::Display for HwAccel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nvenc => write!(f, "NVIDIA NVENC"),
            Self::Vaapi => write!(f, "VAAPI"),
            Self::Qsv => write!(f, "Intel QSV"),
            Self::VideoToolbox => write!(f, "Apple VideoToolbox"),
            Self::Software => write!(f, "Software"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_encoder_h264() {
        assert_eq!(HwAccel::Nvenc.video_encoder(Codec::H264), "h264_nvenc");
        assert_eq!(HwAccel::Vaapi.video_encoder(Codec::H264), "h264_vaapi");
        assert_eq!(HwAccel::Qsv.video_encoder(Codec::H264), "h264_qsv");
        assert_eq!(
            HwAccel::VideoToolbox.video_encoder(Codec::H264),
            "h264_videotoolbox"
        );
        assert_eq!(HwAccel::Software.video_encoder(Codec::H264), "libx264");
    }

    #[test]
    fn test_video_encoder_h265() {
        assert_eq!(HwAccel::Nvenc.video_encoder(Codec::H265), "hevc_nvenc");
        assert_eq!(HwAccel::Vaapi.video_encoder(Codec::H265), "hevc_vaapi");
        assert_eq!(HwAccel::Qsv.video_encoder(Codec::H265), "hevc_qsv");
        assert_eq!(
            HwAccel::VideoToolbox.video_encoder(Codec::H265),
            "hevc_videotoolbox"
        );
        assert_eq!(HwAccel::Software.video_encoder(Codec::H265), "libx265");
    }

    #[test]
    fn test_scale_filter() {
        assert_eq!(HwAccel::Nvenc.scale_filter(), "scale_cuda");
        assert_eq!(HwAccel::Vaapi.scale_filter(), "scale_vaapi");
        assert_eq!(HwAccel::Qsv.scale_filter(), "scale_qsv");
        assert_eq!(HwAccel::Software.scale_filter(), "scale");
    }

    #[test]
    fn test_quality_param() {
        let (name, _) = HwAccel::Nvenc.quality_param(23);
        assert_eq!(name, "-cq");

        let (name, _) = HwAccel::Vaapi.quality_param(23);
        assert_eq!(name, "-qp");

        let (name, _) = HwAccel::Qsv.quality_param(23);
        assert_eq!(name, "-global_quality");

        let (name, _) = HwAccel::Software.quality_param(23);
        assert_eq!(name, "-crf");
    }

    #[test]
    fn test_hwaccel_type() {
        assert_eq!(HwAccel::Nvenc.hwaccel_type(), Some("cuda"));
        assert_eq!(HwAccel::Vaapi.hwaccel_type(), Some("vaapi"));
        assert_eq!(HwAccel::Qsv.hwaccel_type(), Some("qsv"));
        assert_eq!(HwAccel::VideoToolbox.hwaccel_type(), None);
        assert_eq!(HwAccel::Software.hwaccel_type(), None);
    }

    #[test]
    fn test_hwaccel_display() {
        assert_eq!(HwAccel::Nvenc.to_string(), "NVIDIA NVENC");
        assert_eq!(HwAccel::Vaapi.to_string(), "VAAPI");
        assert_eq!(HwAccel::Qsv.to_string(), "Intel QSV");
        assert_eq!(HwAccel::VideoToolbox.to_string(), "Apple VideoToolbox");
        assert_eq!(HwAccel::Software.to_string(), "Software");
    }
}
