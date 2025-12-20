# Cross-Platform Hardware Encoding Support

## Overview

Enable robust hardware-accelerated video encoding across Windows, macOS, and Linux with automatic detection, graceful fallbacks, and reliable FFmpeg discovery.

## Requirements

- **Fast encoding is the priority** - software encoding is a fallback, not a primary path
- **Maximum portability** - cloud, desktop, and self-hosted deployments
- **Graceful degradation** - warn loudly but continue if no hardware encoder found

## Platform Support Matrix

| Platform | Hardware | Encoder | Detection Method |
|----------|----------|---------|------------------|
| macOS | Apple Silicon/Intel | `hevc_videotoolbox` | Compile-time (always available) |
| Linux | NVIDIA | `hevc_nvenc` | `/dev/nvidia*` device check |
| Linux | Intel | `hevc_vaapi` | Render device + FFmpeg probe |
| Linux | AMD | `hevc_vaapi` | Render device + FFmpeg probe |
| Linux | Intel (legacy) | `hevc_qsv` | Render device + FFmpeg probe |
| Windows | NVIDIA | `hevc_nvenc` | DLL check + FFmpeg probe |
| Any | CPU | `libx265` | Always available (fallback) |

## Design

### 1. FFmpeg Discovery Module

New module: `src/util/ffmpeg_discovery.rs`

**Discovery order:**
1. Environment variable (`FFMPEG_PATH` / `FFPROBE_PATH`) - explicit user override
2. Platform-specific common locations:
   - **Windows:** `%LOCALAPPDATA%\ffmpeg\bin`, `C:\ffmpeg\bin`, `.\ffmpeg\`
   - **macOS:** `/opt/homebrew/bin`, `/usr/local/bin`
   - **Linux:** `/usr/bin`, `/usr/local/bin`, `~/.local/bin`
3. PATH lookup (`which ffmpeg` / `where ffmpeg`)
4. Bundled binary (`./bin/ffmpeg` relative to executable)

**Validation:** After finding candidate, run `ffmpeg -version` to verify execution.

```rust
pub struct FfmpegPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl FfmpegPaths {
    /// Discover FFmpeg binaries using platform-specific search
    pub fn discover() -> Result<Self, FfmpegNotFound>;

    /// Validate that required encoders are available
    pub fn validate_encoders(&self, required: &[&str]) -> Result<(), MissingEncoder>;
}
```

### 2. Windows NVENC Detection

Add to `src/video/hwaccel.rs`:

**Phase 1 - Quick DLL check:**
```rust
#[cfg(target_os = "windows")]
fn is_nvidia_available() -> bool {
    let dll_paths = [
        "C:\\Windows\\System32\\nvEncodeAPI64.dll",
        "C:\\Windows\\SysWOW64\\nvEncodeAPI.dll",
    ];
    dll_paths.iter().any(|p| Path::new(p).exists())
}
```

**Phase 2 - FFmpeg probe (only if DLL found):**
```rust
#[cfg(target_os = "windows")]
fn verify_nvenc_works(ffmpeg_path: &Path) -> bool {
    Command::new(ffmpeg_path)
        .args([
            "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "nullsrc=s=64x64:d=0.1",
            "-c:v", "hevc_nvenc",
            "-frames:v", "1",
            "-f", "null", "-"
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

**Detection flow on Windows:**
```
1. Check NVENC DLLs exist (instant)
2. If yes, probe FFmpeg to verify it works (~500ms)
3. If probe fails, fall back to software
```

### 3. Software Fallback Warning

When software encoding is selected, emit prominent warnings:

```rust
if result == Self::Software {
    warn!("========================================");
    warn!("  NO HARDWARE ENCODER DETECTED");
    warn!("  Software encoding will be VERY SLOW");
    warn!("  Processing may take 10-50x longer");
    warn!("========================================");
}
```

**Environment variable override:** `DVM_ALLOW_SOFTWARE_ENCODING=true` suppresses the warning for intentional software-only deployments.

**Startup banner enhancement:**
```
Video Transform DVM v0.1.0
Hardware acceleration: NVIDIA NVENC
```
vs
```
Video Transform DVM v0.1.0
Hardware acceleration: Software (libx265) - WARNING: SLOW
```

### 4. Detection Priority

Detection runs in order, first match wins:

**Linux:**
1. NVIDIA (`/dev/nvidia*` + FFmpeg probe)
2. VAAPI (`/dev/dri/renderD*` + FFmpeg probe) - covers Intel & AMD
3. QSV (`/dev/dri/renderD*` + FFmpeg probe) - Intel legacy
4. Software

**Windows:**
1. NVIDIA (DLL + FFmpeg probe)
2. Software

**macOS:**
1. VideoToolbox (always available, no probe needed)

## File Changes

### New Files
| File | Purpose |
|------|---------|
| `src/util/ffmpeg_discovery.rs` | FFmpeg/FFprobe discovery logic (~150 lines) |

### Modified Files
| File | Changes |
|------|---------|
| `src/video/hwaccel.rs` | Add Windows detection, software warning |
| `src/util/mod.rs` | Export ffmpeg_discovery module |
| `src/config.rs` | Use discovery module instead of manual paths |
| `src/main.rs` | Enhanced startup banner with hwaccel status |

### Unchanged
- `src/video/ffmpeg.rs` - Already uses HwAccel abstraction
- `src/video/transform.rs` - Already hardware-agnostic
- Frontend - No changes needed

## Testing

1. **Unit tests:** Mock filesystem for DLL/device detection
2. **Integration tests:** FFmpeg probe with actual binaries (CI matrix)
3. **Manual testing:** Verify on Windows with/without NVIDIA GPU

## Future Considerations (Out of Scope)

- AMD AMF on Windows - would require significant Windows-specific code
- Raspberry Pi / Rockchip V4L2 - embedded ARM hardware encoders
- AV1 hardware encoding - newer GPUs only, limited compatibility
