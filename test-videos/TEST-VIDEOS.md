# Test Video Fixtures

Test videos for video probe, upload, and playback testing. Each file exists in two variants:

- **Synthetic** (`{name}.ext`) — FFmpeg-generated color bars + text overlay, tiny filesize (<100KB)
- **Real** (`real-{name}.ext`) — Real video content (from YouTube), realistic encoding characteristics

## Spec

| #   | Filename                 | Resolution | Orientation | Codec        | Container | Audio | Purpose                      |
| --- | ------------------------ | ---------- | ----------- | ------------ | --------- | ----- | ---------------------------- |
| 1   | h264-1080p-landscape.mp4 | 1920x1080  | Landscape   | H.264 (avc1) | MP4       | AAC   | Baseline, most common        |
| 2   | h264-720p-landscape.mp4  | 1280x720   | Landscape   | H.264 (avc1) | MP4       | AAC   | Common transcode target      |
| 3   | h264-480p-landscape.mp4  | 854x480    | Landscape   | H.264 (avc1) | MP4       | AAC   | Low quality variant          |
| 4   | h264-360p-landscape.mp4  | 640x360    | Landscape   | H.264 (avc1) | MP4       | AAC   | Lowest transcode target      |
| 5   | h264-240p-landscape.mp4  | 426x240    | Landscape   | H.264 (avc1) | MP4       | AAC   | Minimum quality              |
| 6   | h264-1080p-portrait.mp4  | 1080x1920  | Portrait    | H.264 (avc1) | MP4       | AAC   | Portrait/shorts (kind 34236) |
| 7   | h264-720p-portrait.mp4   | 720x1280   | Portrait    | H.264 (avc1) | MP4       | AAC   | Portrait variant             |
| 8   | hevc-1080p-landscape.mp4 | 1920x1080  | Landscape   | HEVC (hvc1)  | MP4       | AAC   | iOS preferred codec          |
| 9   | hevc-720p-landscape.mp4  | 1280x720   | Landscape   | HEVC (hvc1)  | MP4       | AAC   | iOS HEVC variant             |
| 10  | vp9-1080p-landscape.webm | 1920x1080  | Landscape   | VP9          | WebM      | Opus  | Desktop modern codec         |
| 11  | vp9-720p-landscape.webm  | 1280x720   | Landscape   | VP9          | WebM      | Opus  | VP9 lower quality            |
| 12  | av1-1080p-landscape.mp4  | 1920x1080  | Landscape   | AV1 (av01)   | MP4       | AAC   | Most modern codec            |
| 13  | av1-720p-landscape.mp4   | 1280x720   | Landscape   | AV1 (av01)   | MP4       | AAC   | AV1 variant                  |
| 14  | h264-4k-landscape.mp4    | 3840x2160  | Landscape   | H.264 (avc1) | MP4       | AAC   | 4K label detection           |
| 15  | h264-noaudio-720p.mp4    | 1280x720   | Landscape   | H.264 (avc1) | MP4       | None  | Video-only (no audio track)  |

## Generation Parameters

- **Duration:** 5 seconds
- **Frame rate:** 30fps
- **Synthetic content:** Solid color background + white text overlay showing `{codec} {resolution} {orientation}`
- **Audio (MP4):** 440Hz sine tone at -20dB, AAC (mp4a.40.2), mono
- **Audio (WebM):** 440Hz sine tone at -20dB, Opus, mono
- **Audio (no-audio):** No audio stream at all
- **Tool:** ffmpeg with lavfi filters (no source media needed for synthetic)
- **Target size:** Synthetic <100KB each, Real varies

## File Locations

- **Synthetic files** — committed to git in `src/test/fixtures/videos/`
- **Real files** — gitignored (`real-*`), downloaded on demand from a zip archive

### Downloading real fixtures

```bash
npm run test:fixtures
```

This downloads and extracts `real-*.mp4` / `real-*.webm` files into the fixtures directory.
Tests that require real fixtures are skipped automatically when the files are absent.

<!-- TODO: set REAL_FIXTURES_URL once the zip is hosted -->

## Coverage Matrix

These fixtures cover:

- **Resolution detection:** 240p, 360p, 480p, 720p, 1080p, 4K
- **Orientation detection:** Landscape vs Portrait (for kind 34235 vs 34236)
- **Codec identification:** H.264, HEVC, VP9, AV1
- **Container formats:** MP4, WebM
- **Audio presence:** With audio vs video-only
- **Quality label mapping:** Resolution to human-readable labels (e.g., "1080p", "4K")
