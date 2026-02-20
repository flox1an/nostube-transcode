# Video Transcoding Test Suite Design

**Date:** 2026-02-20
**Status:** Approved

## Problem

We have no integration tests for the video transcoding pipeline. When changing FFmpeg command-building code (e.g., QSV hardware acceleration tweaks), we can't verify whether the changes actually produce correct output on target hardware. The only validation is manual testing.

## Goals

1. Catch regressions in FFmpeg command building (fast, CI-friendly layer)
2. Validate actual transcoding output on real hardware (self-test via web UI)
3. Let anyone running the DVM verify their hardware setup without needing Rust tooling

## Design

### Two-Layer Testing

**Layer 1 — Unit tests (cargo test):**
Existing unit tests for command building (`test_ffmpeg_command_building`, `test_hwaccel_detection`, etc.). These run in CI without video files or hardware. No changes needed here.

**Layer 2 — Self-test via admin command:**
Replace the current single-clip `self_test` admin command with a multi-clip test suite that downloads real video clips from Blossom, transcodes them, and validates output with ffprobe.

### Test Clip Registry

Hardcoded array of `TestClip` definitions baked into the binary:

```rust
struct TestClip {
    name: &'static str,           // e.g. "h264_1080p"
    blossom_hash: &'static str,   // SHA-256, download from configured Blossom servers
    expected_codec: &'static str, // e.g. "h264", "hevc", "av1"
    expected_width: u32,
    expected_height: u32,
    expected_duration_secs: f64,
    has_audio: bool,
    category: TestCategory,       // Standard or EdgeCase
}

enum TestCategory {
    Standard,  // included in quick mode
    EdgeCase,  // included in full mode only
}
```

### Test Clips

**Standard (quick mode):**

| Name | Codec | Resolution | Notes |
|------|-------|-----------|-------|
| `h264_1080p` | H.264 | 1920x1080 | Most common input |
| `h265_1080p` | H.265 | 1920x1080 | HEVC decode path |
| `av1_1080p` | AV1 | 1920x1080 | AV1 decode (N100 problem case) |
| `h264_4k` | H.264 | 3840x2160 | Full resolution ladder |

**Edge cases (full mode adds these):**

| Name | Codec | Resolution | Notes |
|------|-------|-----------|-------|
| `av1_4k` | AV1 | 3840x2160 | AV1 + 4K (stresses decode) |
| `h265_10bit` | H.265 10-bit | 1920x1080 | Pixel format conversion (yuv420p10le -> nv12) |
| `h264_odd` | H.264 | 1918x1078 | Odd dimension rounding (-2 in scale filters) |
| `vp9_720p` | VP9 | 1280x720 | WebM/VP9 software decode |

All clips: 5-10 seconds with audio (except potential "no audio" edge case). Hosted on Blossom, referenced by SHA-256 hash.

### Test Runner

New module: `src/selftest/`

**Flow per test case:**

1. **Download** clip from Blossom (via configured servers, respects cache proxy if configured)
2. **Transcode** using `VideoProcessor` — same API as real jobs
3. **Validate** output with ffprobe:
   - Output file(s) exist and are non-empty
   - Resolution matches expected (even numbers, correct aspect ratio)
   - Codec matches what was requested
   - Duration within 1s tolerance of input
   - Audio stream present when input had audio
4. **Cleanup** temp output files
5. **Report** timing + pass/fail per check

**Test matrix per mode:**

- **Quick mode:** Standard clips only, transcode each to the detected hwaccel's preferred codec, MP4 format only. ~3-4 transcodes.
- **Full mode:** All clips, each transcoded to all available output codecs (H.264, H.265, AV1 if encoder available), both HLS and MP4. Comprehensive coverage.

### Result Structure

```rust
struct TestResult {
    clip_name: String,
    output_codec: String,      // "h264", "h265", "av1"
    output_format: String,     // "mp4" or "hls"
    hwaccel: String,           // "vaapi", "nvenc", etc.
    passed: bool,
    checks: Vec<Check>,
    encode_time_secs: f64,
    speed_ratio: f64,          // video_duration / encode_time
    error: Option<String>,
}

struct Check {
    name: String,              // "resolution", "codec", "duration", "audio"
    passed: bool,
    detail: String,            // "1280x720", "hevc", "5.1s (expected ~5.0s)"
}

struct TestSuiteResult {
    hwaccel: String,
    results: Vec<TestResult>,
    summary: TestSummary,
}

struct TestSummary {
    total: u32,
    passed: u32,
    failed: u32,
    duration_secs: f64,
}
```

### Admin Command

Replace existing `self_test` handler. Keep `system_info` unchanged.

```
Request:  { "method": "self_test", "params": { "mode": "quick" } }
Response: {
    "hwaccel": "vaapi",
    "results": [ ... ],
    "summary": { "total": 4, "passed": 4, "failed": 0, "duration_secs": 12.3 }
}
```

`mode` accepts `"quick"` (default) or `"full"`.

### Frontend

Update `SelfTest.tsx`:

- Mode selector toggle: Quick / Full
- System info section stays (from `system_info` command, loaded on mount)
- Results table: clip name, output codec, format, pass/fail, encode time, speed ratio
- Expandable rows showing individual check details
- Summary bar: X/Y passed, total time, detected hwaccel

### Blossom Cache Proxy

Optional config field for a local Blossom caching proxy (e.g., [flower-cache](https://github.com/hzrd149/flower-cache) or [almond](https://github.com/flox1an/almond)).

When configured, all Blossom fetches (not just test clips) go through:

```
GET http://localhost:24242/<sha256>?as=<original-server>
```

The proxy handles caching transparently. Config field: `blossom_cache_url` in remote config, or `BLOSSOM_CACHE_URL` env var.

This is a separate implementation item that benefits all Blossom operations, not just tests.

## What Changes

| Area | Change |
|------|--------|
| `src/selftest/` (new) | Test clip registry, runner, ffprobe validator |
| `src/admin/handler.rs` | Replace `handle_self_test()` with new runner |
| `src/admin/commands.rs` | Update `SelfTest` params to accept `mode` |
| `frontend/src/components/SelfTest.tsx` | Mode selector, results table, expandable checks |
| `src/blossom/client.rs` | Optional cache proxy support (`?as=` parameter) |

## What Stays Unchanged

- `system_info` admin command
- Admin transport (kind 24207, NIP-44)
- `VideoProcessor` API — test runner calls it like any job
- Existing unit tests

## Open Items

- User will source test video clips (freely licensed, 5-10s each) and upload to Blossom
- Blossom hashes will be filled in once clips are available
- Blossom cache proxy is a separate implementation step
