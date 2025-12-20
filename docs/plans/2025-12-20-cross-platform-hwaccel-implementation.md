# Cross-Platform Hardware Encoding Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable robust hardware-accelerated video encoding on Windows (NVENC) with reliable FFmpeg discovery across all platforms.

**Architecture:** Add `FfmpegDiscovery` module for platform-aware binary discovery, extend `HwAccel::detect()` with Windows support, and add prominent warnings for software fallback.

**Tech Stack:** Rust, FFmpeg, platform-specific APIs (`#[cfg]` conditional compilation)

---

## Task 1: Add FFmpeg Discovery Error Types

**Files:**
- Modify: `src/error.rs`

**Step 1: Add new error variants for FFmpeg discovery**

Add these variants to `ConfigError` in `src/error.rs`:

```rust
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
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/error.rs
git commit -m "feat: add FFmpeg discovery error types"
```

---

## Task 2: Create FFmpeg Discovery Module - Basic Structure

**Files:**
- Create: `src/util/ffmpeg_discovery.rs`
- Modify: `src/util/mod.rs`

**Step 1: Create the discovery module with struct and basic impl**

Create `src/util/ffmpeg_discovery.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

use crate::error::ConfigError;

/// Discovered FFmpeg binary paths
#[derive(Debug, Clone)]
pub struct FfmpegPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl FfmpegPaths {
    /// Discover FFmpeg and FFprobe binaries.
    /// Search order:
    /// 1. Environment variables (FFMPEG_PATH, FFPROBE_PATH)
    /// 2. Platform-specific common locations
    /// 3. System PATH
    pub fn discover() -> Result<Self, ConfigError> {
        let ffmpeg = Self::find_ffmpeg()?;
        let ffprobe = Self::find_ffprobe()?;

        info!(ffmpeg = %ffmpeg.display(), ffprobe = %ffprobe.display(), "FFmpeg binaries discovered");

        Ok(Self { ffmpeg, ffprobe })
    }

    fn find_ffmpeg() -> Result<PathBuf, ConfigError> {
        // 1. Check environment variable
        if let Ok(path) = std::env::var("FFMPEG_PATH") {
            let path = PathBuf::from(path);
            if Self::validate_binary(&path, "ffmpeg") {
                debug!(path = %path.display(), "FFmpeg found via FFMPEG_PATH");
                return Ok(path);
            }
        }

        // 2. Check platform-specific locations
        for path in Self::ffmpeg_search_paths() {
            if Self::validate_binary(&path, "ffmpeg") {
                debug!(path = %path.display(), "FFmpeg found in common location");
                return Ok(path);
            }
        }

        // 3. Check system PATH
        if let Some(path) = Self::find_in_path("ffmpeg") {
            debug!(path = %path.display(), "FFmpeg found in PATH");
            return Ok(path);
        }

        Err(ConfigError::FfmpegNotFound(
            Self::ffmpeg_search_paths()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }

    fn find_ffprobe() -> Result<PathBuf, ConfigError> {
        // 1. Check environment variable
        if let Ok(path) = std::env::var("FFPROBE_PATH") {
            let path = PathBuf::from(path);
            if Self::validate_binary(&path, "ffprobe") {
                debug!(path = %path.display(), "FFprobe found via FFPROBE_PATH");
                return Ok(path);
            }
        }

        // 2. Check platform-specific locations
        for path in Self::ffprobe_search_paths() {
            if Self::validate_binary(&path, "ffprobe") {
                debug!(path = %path.display(), "FFprobe found in common location");
                return Ok(path);
            }
        }

        // 3. Check system PATH
        if let Some(path) = Self::find_in_path("ffprobe") {
            debug!(path = %path.display(), "FFprobe found in PATH");
            return Ok(path);
        }

        Err(ConfigError::FfprobeNotFound(
            Self::ffprobe_search_paths()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }

    /// Validate that a binary exists and is executable
    fn validate_binary(path: &PathBuf, expected_name: &str) -> bool {
        if !path.exists() {
            return false;
        }

        // Run -version to verify it's actually the right binary
        let output = Command::new(path).arg("-version").output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.to_lowercase().contains(expected_name)
            }
            _ => false,
        }
    }

    /// Find a binary in the system PATH
    fn find_in_path(name: &str) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        let name = format!("{}.exe", name);

        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(&name))
                .find(|path| path.exists())
        })
    }

    /// Platform-specific search paths for FFmpeg
    fn ffmpeg_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            // Windows common locations
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                paths.push(PathBuf::from(format!("{}\\ffmpeg\\bin\\ffmpeg.exe", local_app_data)));
            }
            paths.push(PathBuf::from("C:\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from("C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from(".\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from(".\\ffmpeg.exe"));
        }

        #[cfg(target_os = "macos")]
        {
            // macOS Homebrew locations
            paths.push(PathBuf::from("/opt/homebrew/bin/ffmpeg")); // ARM
            paths.push(PathBuf::from("/usr/local/bin/ffmpeg")); // Intel
        }

        #[cfg(target_os = "linux")]
        {
            // Linux common locations
            paths.push(PathBuf::from("/usr/bin/ffmpeg"));
            paths.push(PathBuf::from("/usr/local/bin/ffmpeg"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(format!("{}/.local/bin/ffmpeg", home)));
            }
        }

        paths
    }

    /// Platform-specific search paths for FFprobe
    fn ffprobe_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                paths.push(PathBuf::from(format!("{}\\ffmpeg\\bin\\ffprobe.exe", local_app_data)));
            }
            paths.push(PathBuf::from("C:\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from("C:\\Program Files\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from(".\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from(".\\ffprobe.exe"));
        }

        #[cfg(target_os = "macos")]
        {
            paths.push(PathBuf::from("/opt/homebrew/bin/ffprobe"));
            paths.push(PathBuf::from("/usr/local/bin/ffprobe"));
        }

        #[cfg(target_os = "linux")]
        {
            paths.push(PathBuf::from("/usr/bin/ffprobe"));
            paths.push(PathBuf::from("/usr/local/bin/ffprobe"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(format!("{}/.local/bin/ffprobe", home)));
            }
        }

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_finds_ffmpeg() {
        // This test will pass if FFmpeg is installed on the system
        let result = FfmpegPaths::discover();
        // Don't assert success - FFmpeg might not be installed in CI
        if let Ok(paths) = result {
            assert!(paths.ffmpeg.exists());
            assert!(paths.ffprobe.exists());
        }
    }

    #[test]
    fn test_search_paths_not_empty() {
        let ffmpeg_paths = FfmpegPaths::ffmpeg_search_paths();
        let ffprobe_paths = FfmpegPaths::ffprobe_search_paths();

        assert!(!ffmpeg_paths.is_empty());
        assert!(!ffprobe_paths.is_empty());
    }
}
```

**Step 2: Export module in mod.rs**

Modify `src/util/mod.rs`:

```rust
pub mod ffmpeg_discovery;
pub mod hash;
pub mod temp;

pub use ffmpeg_discovery::FfmpegPaths;
pub use hash::hash_file;
pub use temp::TempDir;
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Run tests**

Run: `cargo test ffmpeg_discovery`
Expected: Tests pass (or skip gracefully if FFmpeg not installed)

**Step 5: Commit**

```bash
git add src/util/ffmpeg_discovery.rs src/util/mod.rs
git commit -m "feat: add FFmpeg discovery module with platform-specific paths"
```

---

## Task 3: Integrate FFmpeg Discovery into Config

**Files:**
- Modify: `src/config.rs`

**Step 1: Update Config to use FfmpegPaths discovery**

Replace the manual ffmpeg/ffprobe path handling in `src/config.rs`:

```rust
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
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: integrate FFmpeg discovery into config"
```

---

## Task 4: Add Windows NVENC Detection

**Files:**
- Modify: `src/video/hwaccel.rs`

**Step 1: Add Windows-specific imports and NVENC detection**

Add the Windows detection block to `src/video/hwaccel.rs`. Insert after the Linux detection functions:

```rust
/// Check if NVIDIA GPU is available (Windows)
/// Phase 1: Quick DLL check
#[cfg(target_os = "windows")]
fn is_nvidia_available() -> bool {
    use tracing::debug;

    // Check for NVENC DLL in system paths
    let dll_paths = [
        "C:\\Windows\\System32\\nvEncodeAPI64.dll",
        "C:\\Windows\\SysWOW64\\nvEncodeAPI.dll",
    ];

    for dll in &dll_paths {
        if Path::new(dll).exists() {
            debug!(dll = %dll, "Found NVENC DLL");
            return true;
        }
    }

    debug!("No NVENC DLL found");
    false
}

/// Verify NVENC actually works via FFmpeg probe (Windows)
/// Phase 2: Functional test
#[cfg(target_os = "windows")]
fn verify_nvenc_works() -> bool {
    use std::process::Command;
    use tracing::debug;

    let result = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=64x64:d=0.1",
            "-c:v",
            "hevc_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            info!("NVENC hardware acceleration verified");
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(stderr = %stderr, "NVENC probe failed");
            false
        }
        Err(e) => {
            debug!(error = %e, "Failed to run FFmpeg NVENC probe");
            false
        }
    }
}
```

**Step 2: Update detect() to include Windows**

Update the `detect()` function in `HwAccel`:

```rust
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

    // Windows: check for NVIDIA
    #[cfg(target_os = "windows")]
    {
        if Self::is_nvidia_available() && Self::verify_nvenc_works() {
            info!("Detected NVIDIA GPU, using NVENC hardware acceleration");
            return Self::Nvenc;
        }
    }

    // Software fallback with warning
    Self::software_fallback()
}

/// Return software encoder with prominent warning
fn software_fallback() -> Self {
    use tracing::warn;

    // Check if user explicitly allows software encoding
    let allow_software = std::env::var("DVM_ALLOW_SOFTWARE_ENCODING")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if allow_software {
        info!("Software encoding enabled via DVM_ALLOW_SOFTWARE_ENCODING");
    } else {
        warn!("========================================");
        warn!("  NO HARDWARE ENCODER DETECTED");
        warn!("  Software encoding (libx265) is SLOW");
        warn!("  Processing may take 10-50x longer");
        warn!("  Set DVM_ALLOW_SOFTWARE_ENCODING=true");
        warn!("  to suppress this warning");
        warn!("========================================");
    }

    Self::Software
}
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Run existing hwaccel tests**

Run: `cargo test hwaccel`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/video/hwaccel.rs
git commit -m "feat: add Windows NVENC detection with FFmpeg probe"
```

---

## Task 5: Add Startup Banner with Hardware Acceleration Status

**Files:**
- Modify: `src/main.rs`

**Step 1: Enhance startup logging**

Update `src/main.rs` to show hardware acceleration in startup banner:

```rust
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::info;

use dvm_video_processing::blossom::{BlobCleanup, BlossomClient};
use dvm_video_processing::config::Config;
use dvm_video_processing::dvm::{AnnouncementPublisher, JobContext, JobHandler};
use dvm_video_processing::nostr::{EventPublisher, SubscriptionManager};
use dvm_video_processing::video::{HwAccel, VideoProcessor};
use dvm_video_processing::web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dvm_video_processing=debug".parse()?),
        )
        .init();

    info!("Starting DVM Video Processing Service");

    let config = Arc::new(Config::from_env()?);

    // Create shared components
    let blossom = Arc::new(BlossomClient::new(config.clone()));
    let processor = Arc::new(VideoProcessor::new(config.clone()));

    // Log startup banner with hardware acceleration status
    let hwaccel = processor.hwaccel();
    log_startup_banner(&config, hwaccel);

    // Channel for job processing
    let (job_tx, job_rx) = mpsc::channel::<JobContext>(100);

    // ... rest of main.rs unchanged ...
```

Add the banner function at the end of the file (before `shutdown_signal`):

```rust
fn log_startup_banner(config: &Config, hwaccel: HwAccel) {
    use dvm_video_processing::video::HwAccel;

    let hwaccel_status = match hwaccel {
        HwAccel::Software => format!("{} - WARNING: SLOW", hwaccel),
        _ => format!("{}", hwaccel),
    };

    info!("╔════════════════════════════════════════╗");
    info!("║      Video Transform DVM               ║");
    info!("╠════════════════════════════════════════╣");
    info!("║ Hardware: {:<28} ║", hwaccel_status);
    info!("║ HTTP Port: {:<27} ║", config.http_port);
    info!("║ Relays: {:<30} ║", config.nostr_relays.len());
    info!("╚════════════════════════════════════════╝");
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add startup banner with hardware acceleration status"
```

---

## Task 6: Add Integration Tests

**Files:**
- Create: `tests/hwaccel_detection.rs`

**Step 1: Create integration test file**

Create `tests/hwaccel_detection.rs`:

```rust
use dvm_video_processing::video::HwAccel;

#[test]
fn test_hwaccel_detection_returns_valid_encoder() {
    let hwaccel = HwAccel::detect();

    // Should always return a valid encoder name
    let encoder = hwaccel.video_encoder();
    assert!(!encoder.is_empty());

    // Encoder should be one of the known values
    let valid_encoders = [
        "hevc_nvenc",
        "hevc_vaapi",
        "hevc_qsv",
        "hevc_videotoolbox",
        "libx265",
    ];
    assert!(valid_encoders.contains(&encoder), "Unknown encoder: {}", encoder);
}

#[test]
fn test_hwaccel_has_valid_scale_filter() {
    let hwaccel = HwAccel::detect();

    let filter = hwaccel.scale_filter();
    assert!(!filter.is_empty());

    let valid_filters = [
        "scale_cuda",
        "scale_vaapi",
        "scale_qsv",
        "scale",
    ];
    assert!(valid_filters.contains(&filter), "Unknown filter: {}", filter);
}

#[test]
fn test_hwaccel_display_not_empty() {
    let hwaccel = HwAccel::detect();
    let display = format!("{}", hwaccel);
    assert!(!display.is_empty());
}
```

**Step 2: Run the tests**

Run: `cargo test hwaccel_detection`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/hwaccel_detection.rs
git commit -m "test: add integration tests for hardware acceleration detection"
```

---

## Task 7: Update Documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update environment variables section**

Add the new environment variable to `CLAUDE.md`:

In the "Environment Variables" section, add under Optional:
```
- `DVM_ALLOW_SOFTWARE_ENCODING` - Set to "true" to suppress slow encoding warning
```

**Step 2: Add platform support section**

Add a new section to `CLAUDE.md`:

```markdown
## Platform Support

### Hardware Acceleration

| Platform | Hardware | Status |
|----------|----------|--------|
| macOS | Apple Silicon/Intel | VideoToolbox (auto-detected) |
| Linux | NVIDIA GPU | NVENC (auto-detected) |
| Linux | Intel/AMD GPU | VAAPI (auto-detected) |
| Linux | Intel (legacy) | QSV (auto-detected) |
| Windows | NVIDIA GPU | NVENC (auto-detected) |
| Any | CPU | libx265 (fallback with warning) |

FFmpeg is automatically discovered in:
- Environment variables (`FFMPEG_PATH`, `FFPROBE_PATH`)
- Platform-specific locations (Homebrew, Program Files, etc.)
- System PATH
```

**Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add platform support and new environment variable"
```

---

## Task 8: Final Verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings

**Step 3: Check formatting**

Run: `cargo fmt --check`
Expected: No formatting issues

**Step 4: Build release**

Run: `cargo build --release`
Expected: Builds successfully

**Step 5: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "chore: final cleanup for cross-platform hwaccel"
```

---

## Summary

**Files created:**
- `src/util/ffmpeg_discovery.rs` (~150 lines)
- `tests/hwaccel_detection.rs` (~40 lines)

**Files modified:**
- `src/error.rs` - Add FFmpeg discovery errors
- `src/util/mod.rs` - Export new module
- `src/config.rs` - Use FFmpeg discovery
- `src/video/hwaccel.rs` - Add Windows detection + software warning
- `src/main.rs` - Add startup banner
- `CLAUDE.md` - Document new features

**Total: ~8 commits, ~250 lines of new code**
