# Codec Selection Feature Design

## Overview

Add user-selectable video codec (H.264 or H.265) with appropriate hardware acceleration per platform. Default codec is H.264 for maximum compatibility.

## Data Model

### Backend (Rust)

New enum in `src/dvm/events.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codec {
    #[default]
    H264,
    H265,
}

impl Codec {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "h265" | "hevc" => Self::H265,
            _ => Self::H264,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
        }
    }
}
```

`JobContext` gains a `codec: Codec` field, parsed from Nostr event tag `["param", "codec", "h264|h265"]`.

### Frontend (TypeScript)

```typescript
export type Codec = "h264" | "h265";
```

Added to `buildTransformRequest()` as `["param", "codec", codec]` tag.

## Hardware Acceleration Mapping

| Platform | H.264 Encoder | H.265 Encoder |
|----------|---------------|---------------|
| macOS VideoToolbox | `h264_videotoolbox` | `hevc_videotoolbox` |
| NVIDIA NVENC | `h264_nvenc` | `hevc_nvenc` |
| Linux VAAPI | `h264_vaapi` | `hevc_vaapi` |
| Intel QSV | `h264_qsv` | `hevc_qsv` |
| Software | `libx264` | `libx265` |

### hwaccel.rs Changes

- `video_encoder(&self, codec: Codec) -> &'static str` - returns platform+codec encoder
- `encoder_options(&self, codec: Codec)` - codec-specific options
- Detection probes remain H.265-based (H.264 support is guaranteed if H.265 works)

## Frontend UI

Codec selector always visible alongside existing options:

```
Output: [MP4] [HLS]    Codec: [H.264] [H.265]    Resolution: [dropdown]
```

- Toggle button style matching Output selector
- Default: H.264
- Available for both MP4 and HLS modes

### Component Changes

**VideoForm.tsx:**
- New state: `codec: Codec` (default "h264")
- New toggle buttons for codec selection
- Updated `onSubmit` signature includes codec

**events.ts:**
- Export `Codec` type
- `buildTransformRequest()` includes codec param tag

## FFmpeg Command Changes

### ffmpeg.rs

- `FfmpegCommand` and `FfmpegMp4Command` store `codec: Codec`
- Pass codec to `hwaccel.video_encoder(codec)`
- `hvc1` tag only added for H.265 (Safari/iOS HEVC compatibility)

### transform.rs

- `VideoProcessor` accepts codec from `JobContext`
- Passes codec to FFmpeg command builders

## Unchanged Components

- Scale filters (codec-independent)
- HLS segmentation options
- Playlist rewriting
- Blossom upload logic

## Files to Modify

1. `src/dvm/events.rs` - Add Codec enum, update JobContext
2. `src/video/hwaccel.rs` - Add codec parameter to encoder methods
3. `src/video/ffmpeg.rs` - Pass codec through, conditional hvc1 tag
4. `src/video/transform.rs` - Thread codec from JobContext
5. `frontend/src/nostr/events.ts` - Add Codec type, update request builder
6. `frontend/src/components/VideoForm.tsx` - Add codec selector UI
