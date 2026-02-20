# Video Transcoding Test Spec

Goal: build a test suite with real video files that exercises all codec/resolution/hwaccel combinations, runnable on different hardware.

## Test Video Requirements

We need a small set of short (5–10 second) test clips covering these dimensions:

### Source Codecs

| # | Codec | Container | Why |
|---|-------|-----------|-----|
| 1 | H.264 (AVC) | .mp4 | Most common input format |
| 2 | H.265 (HEVC) | .mp4 | Increasingly common, tests HEVC decode path |
| 3 | AV1 | .mp4 or .webm | New codec, tests AV1 hardware decode (N100, NVIDIA) |
| 4 | VP9 | .webm | Common YouTube format, software-decode only |

### Source Resolutions

| # | Resolution | Label | Why |
|---|-----------|-------|-----|
| A | 640x360 | 360p | Below all target resolutions — tests "original only" passthrough |
| B | 1280x720 | 720p | Mid-range — tests downscale to 360p + original |
| C | 1920x1080 | 1080p | Standard HD — tests multi-resolution ladder |
| D | 3840x2160 | 4K | Tests full resolution ladder including 4K tier |

### Edge Cases

| # | Description | Why |
|---|-------------|-----|
| E1 | Odd dimensions (e.g., 1918x1078) | Tests `-2` rounding in scale filters |
| E2 | 10-bit color depth (H.265 or AV1) | Tests pixel format conversion (yuv420p10le → nv12) |
| E3 | Portrait/vertical video (1080x1920) | Tests aspect ratio handling in scale filters |
| E4 | No audio track | Tests audio stream mapping when no audio exists |
| E5 | High framerate (60fps) | Tests keyframe/GOP alignment at higher framerates |
| E6 | HDR metadata (HDR10 or HLG) | Tests tone mapping / metadata passthrough |

### Minimum Recommended Set

A practical starting set of **8 clips** that covers the most critical combinations:

| File | Codec | Resolution | Notes |
|------|-------|-----------|-------|
| `h264_1080p.mp4` | H.264 | 1920x1080 | Baseline — most common input |
| `h265_1080p.mp4` | H.265 | 1920x1080 | HEVC decode path |
| `av1_1080p.mp4` | AV1 | 1920x1080 | AV1 decode (the N100 problem case) |
| `vp9_1080p.webm` | VP9 | 1280x720 | WebM/VP9 input |
| `h264_4k.mp4` | H.264 | 3840x2160 | Full resolution ladder |
| `av1_4k.mp4` | AV1 | 3840x2160 | AV1 + 4K (stresses decode) |
| `h265_10bit.mp4` | H.265 10-bit | 1920x1080 | Pixel format conversion |
| `h264_odd.mp4` | H.264 | 1918x1078 | Odd dimension rounding |

Keep clips at **5–10 seconds** with audio. Total size should be manageable (< 100 MB for the set).

## Test Matrix

Each test clip is transcoded with every combination of:

### Output Codec

- H.264
- H.265
- AV1 (when encoder is available)

### Output Format

- HLS (multi-resolution ladder)
- MP4 (single resolution)

### Hardware Acceleration

| Backend | Hardware | Notes |
|---------|----------|-------|
| Software | Any CPU | Baseline reference — always works |
| VAAPI | Intel N100 | Primary production target |
| VAAPI | Intel desktop (e.g., 12th gen+) | Broader Intel coverage |
| QSV | Intel N100 | If/when QSV support is fixed |
| NVENC | NVIDIA GPU | Dedicated GPU path |
| VideoToolbox | macOS (Apple Silicon) | Development machines |

### What to Validate

For each test run, verify:

1. **Exit code** — FFmpeg exits 0
2. **Output exists** — HLS: master.m3u8 + segment files; MP4: output file with non-zero size
3. **Playback** — output plays correctly (ffprobe metadata check at minimum)
4. **Resolution correctness** — output streams have expected dimensions (even numbers, correct aspect ratio)
5. **Codec correctness** — output uses the expected encoder
6. **Duration** — output duration matches input (within 1 second tolerance)
7. **Audio** — audio stream present in output when input has audio

### Performance Metrics (optional, log but don't fail)

- Wall-clock time
- Peak memory usage
- GPU utilization percentage (if available)

## Target Hardware

| Machine | CPU/GPU | OS | Primary backends |
|---------|---------|-----|-----------------|
| N100 server | Intel N100 (Alder Lake-N) | Linux | VAAPI, QSV |
| Dev Mac | Apple M-series | macOS | VideoToolbox |
| NVIDIA box | Any NVIDIA GPU | Linux | NVENC |
| CI runner | Generic x86 | Linux | Software only |

## Test Runner Design (future)

The test runner should:

- Accept a directory of test clips
- Accept a list of hwaccel backends to test (auto-detect available by default)
- Run each clip × output codec × output format × hwaccel combination
- Output a results table (pass/fail + timing)
- Be runnable as `cargo test` integration tests (with feature flag to skip when clips aren't present)
- Support `--hardware` flag to filter which backends to test on current machine

## File Sourcing

Test clips should be:
- Freely licensed (Creative Commons, public domain) or self-generated
- Stored outside the git repo (too large) — downloaded on demand or from a shared location
- A `test-videos/README.md` should document where each clip came from and its license
