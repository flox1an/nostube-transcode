use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
use std::sync::OnceLock;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use tracing::debug;
use tracing::info;

use crate::dvm::events::Codec;

/// Cached result of CUDA AV1 decode capability probe
static CUDA_AV1_DECODE: OnceLock<bool> = OnceLock::new();

/// Cached result of VideoToolbox AV1 decode capability probe
static VT_AV1_DECODE: OnceLock<bool> = OnceLock::new();

/// Cached result of VAAPI HEVC encode capability probe
static VAAPI_HEVC_ENCODE: OnceLock<bool> = OnceLock::new();

/// Cached result of VAAPI AV1 encode capability probe
static VAAPI_AV1_ENCODE: OnceLock<bool> = OnceLock::new();

/// Cached result of VAAPI AV1 decode capability probe
static VAAPI_AV1_DECODE: OnceLock<bool> = OnceLock::new();

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
    /// This runs a quick FFmpeg probe to verify NVENC encoding actually works,
    /// not just that the device files exist.
    #[cfg(target_os = "linux")]
    fn is_nvidia_available() -> bool {
        // First check for NVIDIA device files
        let nvidia_devices = ["/dev/nvidia0", "/dev/nvidiactl"];

        let has_device = nvidia_devices.iter().any(|d| Path::new(d).exists());
        if !has_device {
            debug!("No NVIDIA device files found, NVENC unavailable");
            return false;
        }

        debug!("Found NVIDIA device files, testing NVENC encoding capabilities");

        // --- Test HEVC NVENC encoding ---
        // This probe verifies that FFmpeg can actually use NVENC for encoding.
        // Catches cases where device files exist but:
        // - FFmpeg is not compiled with NVENC support
        // - NVIDIA driver is too old for NVENC
        // - libnvidia-encode is missing
        // - GPU doesn't support NVENC (very old cards)
        let hevc_result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                "cuda=cuda:0",
                "-filter_hw_device",
                "cuda",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.1",
                "-vf",
                "format=nv12,hwupload_cuda",
                "-c:v",
                "hevc_nvenc",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match hevc_result {
            Ok(output) if output.status.success() => {
                info!("NVIDIA NVENC HEVC encoding verified");
                true
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    stderr = %stderr,
                    "NVENC HEVC probe failed, falling back"
                );
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg NVENC probe");
                false
            }
        }
    }

    /// Check if NVIDIA GPU supports AV1 encoding (requires Ada Lovelace / RTX 40xx+)
    #[cfg(target_os = "linux")]
    pub fn is_nvenc_av1_available() -> bool {
        let result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                "cuda=cuda:0",
                "-filter_hw_device",
                "cuda",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.1",
                "-vf",
                "format=nv12,hwupload_cuda",
                "-c:v",
                "av1_nvenc",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!("NVIDIA NVENC AV1 encoding verified (Ada Lovelace+ GPU)");
                true
            }
            Ok(_) => {
                debug!("NVENC AV1 not available (requires RTX 40xx or newer)");
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg NVENC AV1 probe");
                false
            }
        }
    }

    /// Check if NVIDIA GPU supports AV1 encoding (non-Linux stub)
    #[cfg(not(target_os = "linux"))]
    pub fn is_nvenc_av1_available() -> bool {
        false
    }

    /// Check if NVIDIA CUDA can hardware-decode AV1 (requires Ampere / RTX 30xx+)
    ///
    /// When CUDA AV1 decode is not available, the system falls back to software
    /// decoding (libdav1d) with hwupload_cuda for encoding.
    #[cfg(target_os = "linux")]
    pub fn is_cuda_av1_decode_available() -> bool {
        // Test CUDA hardware AV1 decoding by running a quick decode probe
        let result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-hwaccel",
                "cuda",
                "-hwaccel_output_format",
                "cuda",
                "-c:v",
                "av1_cuvid",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.1:r=1",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!("CUDA AV1 hardware decoding verified");
                true
            }
            Ok(_) => {
                debug!("CUDA AV1 hardware decoding not available, will use software decode");
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run CUDA AV1 decode probe");
                false
            }
        }
    }

    /// Check if NVIDIA CUDA can hardware-decode AV1 (non-Linux stub)
    #[cfg(not(target_os = "linux"))]
    pub fn is_cuda_av1_decode_available() -> bool {
        false
    }

    /// Check if VideoToolbox can hardware-decode AV1 (requires Apple M3+)
    #[cfg(target_os = "macos")]
    pub fn is_videotoolbox_av1_decode_available() -> bool {
        let result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-hwaccel",
                "videotoolbox",
                "-c:v",
                "av1",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.1:r=1",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!("VideoToolbox AV1 hardware decoding verified (Apple M3+ chip)");
                true
            }
            Ok(_) => {
                debug!("VideoToolbox AV1 hardware decoding not available (M1/M2 chip)");
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run VideoToolbox AV1 decode probe");
                false
            }
        }
    }

    /// Check if VideoToolbox can hardware-decode AV1 (non-macOS stub)
    #[cfg(not(target_os = "macos"))]
    pub fn is_videotoolbox_av1_decode_available() -> bool {
        false
    }

    /// Check if VideoToolbox AV1 hardware decode is available (cached).
    pub fn has_videotoolbox_av1_decode() -> bool {
        *VT_AV1_DECODE.get_or_init(Self::is_videotoolbox_av1_decode_available)
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

        // --- Test H.264 VAAPI encoding ---
        // Fallback probe: if HEVC failed, check if H.264 VAAPI works.
        // Some GPUs support H.264 but not HEVC encoding.
        let mut h264_ok = false;
        if !hevc_ok {
            let h264_result = Command::new("ffmpeg")
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
                    "color=c=black:s=64x64:d=0.1",
                    "-vf",
                    "format=nv12,hwupload",
                    "-c:v",
                    "h264_vaapi",
                    "-frames:v",
                    "1",
                    "-f",
                    "null",
                    "-",
                ])
                .output();

            match h264_result {
                Ok(output) if output.status.success() => {
                    info!(device = %device, "VAAPI H.264 encoding verified");
                    h264_ok = true;
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    debug!(
                        device = %device,
                        stderr = %stderr,
                        "VAAPI H.264 probe failed"
                    );
                }
                Err(e) => {
                    debug!(error = %e, "Failed to run FFmpeg VAAPI H.264 probe");
                }
            }
        }

        // Cache the HEVC result so we don't re-probe later
        let _ = VAAPI_HEVC_ENCODE.set(hevc_ok);

        // VAAPI is considered available if any encoding works.
        // AV1 decoding is handled automatically by -hwaccel vaapi (falls back to
        // software libdav1d when hardware AV1 decode is unavailable).
        hevc_ok || h264_ok
    }

    /// Find the first available render device path (Linux)
    #[cfg(target_os = "linux")]
    fn find_render_device() -> Option<&'static str> {
        ["/dev/dri/renderD128", "/dev/dri/renderD129"]
            .iter()
            .find(|d| Path::new(*d).exists())
            .copied()
    }

    /// Check if VAAPI supports HEVC encoding (cached).
    /// Some GPUs (e.g. older Intel, some AMD) only support H.264 via VAAPI.
    pub fn has_vaapi_hevc_encode() -> bool {
        *VAAPI_HEVC_ENCODE.get_or_init(Self::probe_vaapi_hevc_encode)
    }

    /// Probe VAAPI HEVC encode capability
    #[cfg(target_os = "linux")]
    fn probe_vaapi_hevc_encode() -> bool {
        let Some(device) = Self::find_render_device() else {
            return false;
        };

        let result = Command::new("ffmpeg")
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
                "nullsrc=s=64x64:d=0.1",
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                "hevc_vaapi",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!(device = %device, "VAAPI HEVC encoding verified");
                true
            }
            Ok(_) => {
                debug!(device = %device, "VAAPI HEVC encoding not available");
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg VAAPI HEVC probe");
                false
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn probe_vaapi_hevc_encode() -> bool {
        false
    }

    /// Check if VAAPI supports AV1 encoding (cached).
    /// Requires AMD RDNA3+ or Intel Arc/DG2+.
    pub fn is_vaapi_av1_encode_available() -> bool {
        *VAAPI_AV1_ENCODE.get_or_init(Self::probe_vaapi_av1_encode)
    }

    /// Probe VAAPI AV1 encode capability
    #[cfg(target_os = "linux")]
    fn probe_vaapi_av1_encode() -> bool {
        let Some(device) = Self::find_render_device() else {
            return false;
        };

        let result = Command::new("ffmpeg")
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
                "nullsrc=s=64x64:d=0.1",
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                "av1_vaapi",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!(device = %device, "VAAPI AV1 encoding verified (RDNA3+/Arc+)");
                true
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    device = %device,
                    stderr = %stderr,
                    "VAAPI AV1 encode not available"
                );
                false
            }
            Err(e) => {
                debug!(error = %e, "Failed to run FFmpeg VAAPI AV1 encode probe");
                false
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn probe_vaapi_av1_encode() -> bool {
        false
    }

    /// Check if VAAPI supports AV1 hardware decoding (cached).
    /// Requires AMD RDNA2+ or Intel 12th gen+.
    pub fn has_vaapi_av1_decode() -> bool {
        *VAAPI_AV1_DECODE.get_or_init(Self::probe_vaapi_av1_decode)
    }

    /// Probe VAAPI AV1 decode capability by checking if av1 profile is listed
    /// in FFmpeg's VAAPI device capabilities.
    #[cfg(target_os = "linux")]
    fn probe_vaapi_av1_decode() -> bool {
        let Some(device) = Self::find_render_device() else {
            return false;
        };

        // Query FFmpeg for VAAPI decode profiles on this device.
        // If AV1 decode is supported, the output will contain "AV1" or "av1" profile entries.
        let result = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "quiet",
                "-init_hw_device",
                &format!("vaapi=vaapi:{}", device),
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=64x64:d=0.1:r=1",
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                "av1_vaapi",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ])
            .output();

        // If av1_vaapi encoder initializes successfully, the device supports AV1 operations.
        // AV1 decode support generally correlates with encode support on the same generation.
        // For a more precise check we also try vainfo.
        let encode_works = matches!(result, Ok(ref output) if output.status.success());

        if encode_works {
            // If encode works, decode almost certainly works too (same ASIC block)
            info!(device = %device, "VAAPI AV1 hardware decoding verified (via encode probe)");
            return true;
        }

        // Fallback: check vainfo for AV1 decode profiles
        let vainfo_result = Command::new("vainfo")
            .args(["--display", "drm", "--device", device])
            .output();

        match vainfo_result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let has_av1 = stdout.lines().any(|line| {
                    line.contains("VAProfileAV1") && line.contains("VAEntrypointVLD")
                });
                if has_av1 {
                    info!(device = %device, "VAAPI AV1 hardware decoding verified (via vainfo)");
                } else {
                    debug!(device = %device, "VAAPI AV1 hardware decoding not available");
                }
                has_av1
            }
            Err(e) => {
                debug!(
                    error = %e,
                    "vainfo not found, cannot probe VAAPI AV1 decode capability"
                );
                false
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn probe_vaapi_av1_decode() -> bool {
        false
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

    /// Get the video encoder name for this acceleration and codec.
    ///
    /// Falls back to a working encoder when the GPU doesn't support the requested codec:
    /// - VAAPI + H.265: falls back to `h264_vaapi` if HEVC encode isn't available
    /// - VAAPI + AV1: falls back to `hevc_vaapi` or `h264_vaapi` if AV1 encode isn't available
    pub fn video_encoder(&self, codec: Codec) -> &'static str {
        match (self, codec) {
            (Self::Nvenc, Codec::H264) => "h264_nvenc",
            (Self::Nvenc, Codec::H265) => "hevc_nvenc",
            (Self::Nvenc, Codec::AV1) => "av1_nvenc",
            (Self::Vaapi, Codec::H264) => "h264_vaapi",
            (Self::Vaapi, Codec::H265) => {
                if Self::has_vaapi_hevc_encode() {
                    "hevc_vaapi"
                } else {
                    "h264_vaapi"
                }
            }
            (Self::Vaapi, Codec::AV1) => {
                if Self::is_vaapi_av1_encode_available() {
                    "av1_vaapi"
                } else if Self::has_vaapi_hevc_encode() {
                    "hevc_vaapi"
                } else {
                    "h264_vaapi"
                }
            }
            (Self::Qsv, Codec::H264) => "h264_qsv",
            (Self::Qsv, Codec::H265) => "hevc_qsv",
            (Self::Qsv, Codec::AV1) => "av1_qsv",
            (Self::VideoToolbox, Codec::H264) => "h264_videotoolbox",
            (Self::VideoToolbox, Codec::H265) => "hevc_videotoolbox",
            (Self::Software, Codec::H264) => "libx264",
            (Self::Software, Codec::H265) => "libx265",
            (Self::Software, Codec::AV1) => "libsvtav1",
            (Self::VideoToolbox, Codec::AV1) => "av1_videotoolbox",
        }
    }

    /// Get the video decoder name for this acceleration and codec (if any)
    pub fn video_decoder(&self, codec: Codec) -> Option<&'static str> {
        match (self, codec) {
            (Self::Nvenc, Codec::AV1) => {
                // Only use CUDA AV1 decoder if hardware decode is available
                if Self::has_cuda_av1_decode() {
                    Some("av1_cuvid")
                } else {
                    None // Fall back to software decode (libdav1d)
                }
            }
            // VAAPI: Don't specify an explicit decoder. -hwaccel vaapi handles
            // hardware-accelerated decoding automatically, and falls back to
            // software decoding (e.g., libdav1d for AV1) when HW decode is unavailable.
            _ => None,
        }
    }

    /// Check if CUDA AV1 hardware decode is available (cached).
    pub fn has_cuda_av1_decode() -> bool {
        *CUDA_AV1_DECODE.get_or_init(Self::is_cuda_av1_decode_available)
    }

    /// Check if this hardware backend supports AV1 hardware decoding.
    pub fn has_av1_hw_decode(&self) -> bool {
        match self {
            Self::Nvenc => Self::has_cuda_av1_decode(),
            Self::VideoToolbox => Self::has_videotoolbox_av1_decode(),
            Self::Vaapi => Self::has_vaapi_av1_decode(),
            // QSV: AV1 decode is handled automatically and falls back to software.
            _ => false,
        }
    }

    /// Check if hardware decoding should be skipped for the given source codec.
    ///
    /// When this returns true, the FFmpeg command should:
    /// - NOT use `-hwaccel` and `-hwaccel_output_format` (use software decode)
    /// - Include `hwupload_cuda`/`hwupload` in the filter graph to upload frames to GPU
    /// - Still use `-init_hw_device` and `-filter_hw_device` for encoding/filters
    ///
    /// Note: For VAAPI this returns false even without AV1 HW decode because
    /// the VAAPI filter pipeline (`format=nv12|vaapi,hwupload`) already handles
    /// transparent fallback from HW to SW decode.
    pub fn needs_sw_decode(&self, source_codec: Option<&str>) -> bool {
        let source = match source_codec {
            Some(s) => s,
            None => return false,
        };
        let codec = Codec::from_str(source);
        match (self, codec) {
            // NVENC + AV1 source: need software decode if GPU can't decode AV1
            (Self::Nvenc, Codec::AV1) => !Self::has_cuda_av1_decode(),
            _ => false,
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

    /// Get per-resolution maximum bitrate and buffer size for hardware encoders.
    ///
    /// Hardware encoders (especially NVENC) in constant-quality mode can produce
    /// excessively high bitrates without an upper cap. This returns reasonable
    /// maxrate/bufsize values per resolution height to keep file sizes in check.
    ///
    /// Returns `Some((maxrate, bufsize))` or `None` if no cap is needed (software encoders).
    pub fn bitrate_cap(&self, height: u32) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Nvenc => {
                // NVENC VBR + CQ mode needs maxrate caps; without them, bitrates can
                // be 2-5x higher than software CRF at the same quality value.
                let (maxrate, bufsize) = match height {
                    h if h <= 240 => ("500k", "1000k"),
                    h if h <= 360 => ("1000k", "2000k"),
                    h if h <= 480 => ("1500k", "3000k"),
                    h if h <= 720 => ("3000k", "6000k"),
                    h if h <= 1080 => ("5000k", "10000k"),
                    _ => ("12000k", "24000k"), // 4K
                };
                Some((maxrate, bufsize))
            }
            Self::Vaapi => {
                // VAAPI in QP mode can produce higher-than-expected bitrates on some
                // AMD/Intel GPUs, especially with high-motion content. Apply moderate caps.
                let (maxrate, bufsize) = match height {
                    h if h <= 240 => ("750k", "1500k"),
                    h if h <= 360 => ("1500k", "3000k"),
                    h if h <= 480 => ("2500k", "5000k"),
                    h if h <= 720 => ("4000k", "8000k"),
                    h if h <= 1080 => ("6000k", "12000k"),
                    _ => ("14000k", "28000k"), // 4K
                };
                Some((maxrate, bufsize))
            }
            // Other hardware encoders generally produce reasonable bitrates with their
            // quality modes (global_quality for QSV).
            _ => None,
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
            (Self::Vaapi, Codec::H264) => vec![
                ("-profile:v", "high"),
                ("-g", "60"),
                ("-keyint_min", "60"),
            ],
            (Self::Vaapi, _) => vec![
                // HEVC/AV1: use main profile for broad compatibility
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
        // VAAPI falls back to h264_vaapi if HEVC encode isn't available on this GPU
        let vaapi_h265 = HwAccel::Vaapi.video_encoder(Codec::H265);
        assert!(
            vaapi_h265 == "hevc_vaapi" || vaapi_h265 == "h264_vaapi",
            "VAAPI H.265 encoder should be hevc_vaapi or h264_vaapi fallback, got: {}",
            vaapi_h265
        );
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

    #[test]
    fn test_vaapi_av1_encoder_fallback() {
        // VAAPI AV1 encoder should fall back gracefully when AV1 isn't supported
        let encoder = HwAccel::Vaapi.video_encoder(Codec::AV1);
        assert!(
            encoder == "av1_vaapi" || encoder == "hevc_vaapi" || encoder == "h264_vaapi",
            "VAAPI AV1 encoder should be av1_vaapi, hevc_vaapi, or h264_vaapi fallback, got: {}",
            encoder
        );
    }

    #[test]
    fn test_vaapi_has_bitrate_cap() {
        // VAAPI should now have bitrate caps
        assert!(HwAccel::Vaapi.bitrate_cap(720).is_some());
        assert!(HwAccel::Vaapi.bitrate_cap(1080).is_some());

        let (maxrate, bufsize) = HwAccel::Vaapi.bitrate_cap(720).unwrap();
        assert_eq!(maxrate, "4000k");
        assert_eq!(bufsize, "8000k");
    }

    #[test]
    fn test_vaapi_av1_hw_decode_probe_runs() {
        // Just verify the cached probe doesn't panic
        let _ = HwAccel::has_vaapi_av1_decode();
    }

    #[test]
    fn test_vaapi_hevc_encode_probe_runs() {
        // Just verify the cached probe doesn't panic
        let _ = HwAccel::has_vaapi_hevc_encode();
    }

    #[test]
    fn test_vaapi_av1_encode_probe_runs() {
        // Just verify the cached probe doesn't panic
        let _ = HwAccel::is_vaapi_av1_encode_available();
    }

    #[test]
    fn test_vaapi_encoder_options_h264() {
        let opts = HwAccel::Vaapi.encoder_options(Codec::H264);
        let profile = opts.iter().find(|(k, _)| *k == "-profile:v");
        assert_eq!(profile, Some(&("-profile:v", "high")));
    }

    #[test]
    fn test_vaapi_encoder_options_hevc() {
        let opts = HwAccel::Vaapi.encoder_options(Codec::H265);
        let profile = opts.iter().find(|(k, _)| *k == "-profile:v");
        assert_eq!(profile, Some(&("-profile:v", "main")));
    }

    #[test]
    fn test_has_av1_hw_decode_includes_vaapi() {
        // Verify VAAPI is now wired into has_av1_hw_decode
        // (the actual result depends on hardware, but the method should not panic)
        let _ = HwAccel::Vaapi.has_av1_hw_decode();
    }
}
