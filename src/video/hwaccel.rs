use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use tracing::debug;
use tracing::info;

/// Hardware acceleration backend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HwAccel {
    /// NVIDIA NVENC (Linux/Windows)
    Nvenc,
    /// Intel Quick Sync Video (Linux)
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

        // Linux: check for NVIDIA first (usually faster), then Intel QSV
        #[cfg(target_os = "linux")]
        {
            if Self::is_nvidia_available() {
                info!("Detected NVIDIA GPU, using NVENC hardware acceleration");
                return Self::Nvenc;
            }

            if Self::is_qsv_available() {
                info!("Detected Intel QSV hardware acceleration");
                return Self::Qsv;
            }
        }

        info!("No hardware acceleration detected, using software encoding");
        Self::Software
    }

    /// Check if NVIDIA GPU is available (Linux)
    #[cfg(target_os = "linux")]
    fn is_nvidia_available() -> bool {
        // Check for NVIDIA device
        let nvidia_devices = [
            "/dev/nvidia0",
            "/dev/nvidiactl",
        ];

        for device in &nvidia_devices {
            if Path::new(device).exists() {
                info!(device = %device, "Found NVIDIA device");
                return true;
            }
        }

        false
    }

    /// Check if Intel QSV is available (Linux)
    /// This runs a quick FFmpeg probe to verify QSV actually works,
    /// not just that the render device exists (which could be AMD or unsupported Intel).
    #[cfg(target_os = "linux")]
    fn is_qsv_available() -> bool {
        // First check for render device
        let render_devices = ["/dev/dri/renderD128", "/dev/dri/renderD129"];

        let device = render_devices
            .iter()
            .find(|d| Path::new(*d).exists());

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

    /// Get the video encoder name for this acceleration
    pub fn video_encoder(&self) -> &'static str {
        match self {
            Self::Nvenc => "hevc_nvenc",
            Self::Qsv => "hevc_qsv",
            Self::VideoToolbox => "hevc_videotoolbox",
            Self::Software => "libx265",
        }
    }

    /// Get the scale filter name for this acceleration
    pub fn scale_filter(&self) -> &'static str {
        match self {
            Self::Nvenc => "scale_cuda",
            Self::Qsv => "scale_qsv",
            Self::VideoToolbox => "scale",
            Self::Software => "scale",
        }
    }

    /// Whether this uses hardware-accelerated decoding
    pub fn uses_hw_decode(&self) -> bool {
        matches!(self, Self::Nvenc | Self::Qsv)
    }

    /// Get hwaccel type for FFmpeg -hwaccel option
    pub fn hwaccel_type(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("cuda"),
            Self::Qsv => Some("qsv"),
            _ => None,
        }
    }

    /// Get hwaccel output format for FFmpeg -hwaccel_output_format option
    /// This keeps decoded frames in GPU memory for efficient hardware encoding.
    pub fn hwaccel_output_format(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("cuda"),
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
            Self::Software => {
                ("-crf", crf.to_string())
            }
        }
    }

    /// Get additional encoder options
    pub fn encoder_options(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::Nvenc => vec![
                ("-preset", "p4"),  // balanced preset
                ("-tune", "hq"),
                ("-rc", "vbr"),
            ],
            Self::Qsv => vec![
                ("-preset", "medium"),
            ],
            Self::VideoToolbox => vec![],
            Self::Software => vec![
                ("-preset", "medium"),
            ],
        }
    }

    /// Get init_hw_device option for complex filter graphs
    pub fn init_hw_device(&self) -> Option<String> {
        match self {
            Self::Nvenc => Some("cuda=cuda:0".to_string()),
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
            Self::Qsv => Some("qsv"),
            _ => None,
        }
    }

    /// Get hardware upload filter for transitioning from software to hardware frames
    pub fn upload_filter(&self) -> Option<&'static str> {
        match self {
            Self::Nvenc => Some("hwupload_cuda"),
            // QSV: Convert to nv12 (required by QSV) and upload to QSV memory.
            // Use scale filter instead of format filter because scale can handle
            // 10-bit to 8-bit conversion (e.g., AV1 decoded by libdav1d outputs yuv420p10le).
            // The format filter alone cannot convert between different bit depths.
            // extra_hw_frames=64 provides buffer for frame reordering during encoding.
            Self::Qsv => Some("scale=format=nv12,hwupload=extra_hw_frames=64"),
            _ => None,
        }
    }
}

impl std::fmt::Display for HwAccel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nvenc => write!(f, "NVIDIA NVENC"),
            Self::Qsv => write!(f, "Intel QSV"),
            Self::VideoToolbox => write!(f, "Apple VideoToolbox"),
            Self::Software => write!(f, "Software (libx265)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_encoder() {
        assert_eq!(HwAccel::Nvenc.video_encoder(), "hevc_nvenc");
        assert_eq!(HwAccel::Qsv.video_encoder(), "hevc_qsv");
        assert_eq!(HwAccel::VideoToolbox.video_encoder(), "hevc_videotoolbox");
        assert_eq!(HwAccel::Software.video_encoder(), "libx265");
    }

    #[test]
    fn test_scale_filter() {
        assert_eq!(HwAccel::Nvenc.scale_filter(), "scale_cuda");
        assert_eq!(HwAccel::Qsv.scale_filter(), "scale_qsv");
        assert_eq!(HwAccel::Software.scale_filter(), "scale");
    }

    #[test]
    fn test_quality_param() {
        let (name, _) = HwAccel::Nvenc.quality_param(23);
        assert_eq!(name, "-cq");

        let (name, _) = HwAccel::Qsv.quality_param(23);
        assert_eq!(name, "-global_quality");

        let (name, _) = HwAccel::Software.quality_param(23);
        assert_eq!(name, "-crf");
    }

    #[test]
    fn test_hwaccel_type() {
        assert_eq!(HwAccel::Nvenc.hwaccel_type(), Some("cuda"));
        assert_eq!(HwAccel::Qsv.hwaccel_type(), Some("qsv"));
        assert_eq!(HwAccel::VideoToolbox.hwaccel_type(), None);
        assert_eq!(HwAccel::Software.hwaccel_type(), None);
    }
}
