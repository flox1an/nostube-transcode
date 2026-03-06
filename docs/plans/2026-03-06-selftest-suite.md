# Self-Test Suite Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the single-clip self-test with a multi-clip test suite using local video fixtures, runnable via admin command with quick/full modes.

**Architecture:** `src/selftest/` module with clip registry, test runner, and ffprobe validator. Admin command dispatches to runner, returns per-clip results. Frontend shows results table with mode toggle.

**Tech Stack:** Rust (tokio, serde), FFmpeg/ffprobe for validation, React/TypeScript frontend

---

### Task 1: Copy test video fixtures

**Files:**
- Create: `test-videos/` directory (copy 15 files from `../nostube/src/test/fixtures/videos/`)
- Create: `test-videos/TEST-VIDEOS.md` (copy from same location)
- Modify: `.gitignore` — add `test-videos/real-*`

**Step 1: Copy fixtures and spec**

```bash
mkdir -p test-videos
cp ../nostube/src/test/fixtures/videos/*.mp4 test-videos/
cp ../nostube/src/test/fixtures/videos/*.webm test-videos/
cp ../nostube/src/test/fixtures/videos/TEST-VIDEOS.md test-videos/
```

**Step 2: Add gitignore for future real clips**

Append to `.gitignore`:
```
# Test video fixtures - real clips downloaded on demand
test-videos/real-*
```

**Step 3: Verify files**

```bash
ls -la test-videos/
# Should show 15 video files + TEST-VIDEOS.md, ~864KB total
```

**Step 4: Commit**

```bash
git add test-videos/ .gitignore
git commit -m "feat: add synthetic test video fixtures (15 clips, 864KB)"
```

---

### Task 2: Create selftest clip registry (`src/selftest/mod.rs`)

**Files:**
- Create: `src/selftest/mod.rs`
- Modify: `src/lib.rs` — add `pub mod selftest;`

**Step 1: Write the clip registry module**

`src/selftest/mod.rs` defines:

```rust
pub mod runner;
pub mod validate;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCategory {
    Standard,
    EdgeCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMode {
    Quick,
    Full,
}

impl TestMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "full" => Self::Full,
            _ => Self::Quick,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestClip {
    pub name: &'static str,
    pub filename: &'static str,
    pub expected_codec: &'static str,
    pub expected_width: u32,
    pub expected_height: u32,
    pub has_audio: bool,
    pub category: TestCategory,
}

/// All registered test clips.
pub const TEST_CLIPS: &[TestClip] = &[
    // Standard clips (quick mode)
    TestClip {
        name: "h264_1080p",
        filename: "h264-1080p-landscape.mp4",
        expected_codec: "h264",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "hevc_1080p",
        filename: "hevc-1080p-landscape.mp4",
        expected_codec: "hevc",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "av1_1080p",
        filename: "av1-1080p-landscape.mp4",
        expected_codec: "av1",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "h264_4k",
        filename: "h264-4k-landscape.mp4",
        expected_codec: "h264",
        expected_width: 3840,
        expected_height: 2160,
        has_audio: true,
        category: TestCategory::Standard,
    },
    // Edge case clips (full mode)
    TestClip {
        name: "vp9_1080p",
        filename: "vp9-1080p-landscape.webm",
        expected_codec: "vp9",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_portrait",
        filename: "h264-1080p-portrait.mp4",
        expected_codec: "h264",
        expected_width: 1080,
        expected_height: 1920,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_noaudio",
        filename: "h264-noaudio-720p.mp4",
        expected_codec: "h264",
        expected_width: 1280,
        expected_height: 720,
        has_audio: false,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_240p",
        filename: "h264-240p-landscape.mp4",
        expected_codec: "h264",
        expected_width: 426,
        expected_height: 240,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "vp9_720p",
        filename: "vp9-720p-landscape.webm",
        expected_codec: "vp9",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "hevc_720p",
        filename: "hevc-720p-landscape.mp4",
        expected_codec: "hevc",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "av1_720p",
        filename: "av1-720p-landscape.mp4",
        expected_codec: "av1",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
];

/// Get clips for a given test mode.
pub fn clips_for_mode(mode: TestMode) -> Vec<&'static TestClip> {
    TEST_CLIPS
        .iter()
        .filter(|c| mode == TestMode::Full || c.category == TestCategory::Standard)
        .collect()
}
```

**Step 2: Register module in lib.rs**

Add `pub mod selftest;` to `src/lib.rs`.

**Step 3: Verify it compiles**

```bash
cargo check
```

**Step 4: Commit**

```bash
git add src/selftest/mod.rs src/lib.rs
git commit -m "feat: add selftest clip registry with quick/full modes"
```

---

### Task 3: Create ffprobe validator (`src/selftest/validate.rs`)

**Files:**
- Create: `src/selftest/validate.rs`

This module validates FFmpeg output using ffprobe. Each check returns a `Check` struct.

```rust
use crate::video::VideoMetadata;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Validate that the output file exists and is non-empty.
pub fn check_output_exists(path: &Path) -> Check {
    let exists = path.exists();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Check {
        name: "output_exists".into(),
        passed: exists && size > 0,
        detail: if exists {
            format!("{} bytes", size)
        } else {
            "file not found".into()
        },
    }
}

/// Validate output resolution. Accepts the output height; checks it is even
/// and does not exceed the source height.
pub fn check_resolution(
    metadata: &VideoMetadata,
    max_height: u32,
) -> Check {
    let (w, h) = metadata.resolution().unwrap_or((0, 0));
    let even = w % 2 == 0 && h % 2 == 0;
    let within_bounds = h <= max_height;
    Check {
        name: "resolution".into(),
        passed: w > 0 && h > 0 && even && within_bounds,
        detail: format!("{}x{} (max_h={}, even={})", w, h, max_height, even),
    }
}

/// Validate output codec matches the requested codec.
pub fn check_codec(metadata: &VideoMetadata, expected: &str) -> Check {
    let actual = metadata
        .video_stream()
        .and_then(|s| s.codec_name.clone())
        .unwrap_or_default();
    // Normalize: "hevc" matches "h265", "h264" matches "h264"
    let passed = match expected {
        "h264" => actual == "h264",
        "h265" | "hevc" => actual == "hevc" || actual == "h265",
        "av1" => actual == "av1",
        _ => actual == expected,
    };
    Check {
        name: "codec".into(),
        passed,
        detail: format!("expected={}, actual={}", expected, actual),
    }
}

/// Validate output duration is within tolerance of the source.
pub fn check_duration(metadata: &VideoMetadata, expected_secs: f64, tolerance: f64) -> Check {
    let actual = metadata.duration_secs().unwrap_or(0.0);
    let diff = (actual - expected_secs).abs();
    Check {
        name: "duration".into(),
        passed: diff <= tolerance,
        detail: format!("{:.1}s (expected ~{:.1}s, diff={:.1}s)", actual, expected_secs, diff),
    }
}

/// Validate audio stream presence matches expectation.
pub fn check_audio(metadata: &VideoMetadata, expected_has_audio: bool) -> Check {
    let has_audio = metadata.audio_stream().is_some();
    Check {
        name: "audio".into(),
        passed: has_audio == expected_has_audio,
        detail: format!(
            "has_audio={}, expected={}",
            has_audio, expected_has_audio
        ),
    }
}
```

**Step 1:** Write the file as above.

**Step 2: Verify it compiles**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/selftest/validate.rs
git commit -m "feat: add ffprobe-based output validator for selftest"
```

---

### Task 4: Create test runner (`src/selftest/runner.rs`)

**Files:**
- Create: `src/selftest/runner.rs`

The runner orchestrates per-clip transcodes and validation. It receives a `Config` and mode, runs clips, returns structured results.

```rust
use crate::config::Config;
use crate::dvm::events::{Codec, Resolution};
use crate::selftest::validate::*;
use crate::selftest::{clips_for_mode, TestClip, TestMode};
use crate::video::{VideoMetadata, VideoProcessor};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub clip_name: String,
    pub output_codec: String,
    pub hwaccel: String,
    pub passed: bool,
    pub checks: Vec<Check>,
    pub encode_time_secs: f64,
    pub speed_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestSuiteResult {
    pub hwaccel: String,
    pub mode: String,
    pub results: Vec<TestResult>,
    pub summary: TestSummary,
}

/// Resolve the test-videos directory. Checks:
/// 1. `./test-videos/` (relative to cwd)
/// 2. Next to the binary
fn find_test_videos_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let candidate = cwd.join("test-videos");
    if candidate.is_dir() {
        return Some(candidate);
    }
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join("test-videos");
    if candidate.is_dir() {
        return Some(candidate);
    }
    None
}

/// Run the self-test suite.
pub async fn run_test_suite(
    config: Arc<Config>,
    mode: TestMode,
) -> TestSuiteResult {
    let suite_start = Instant::now();
    let processor = VideoProcessor::new(config.clone());
    let hwaccel = processor.hwaccel();
    let hwaccel_str = hwaccel.to_string();

    let clips = clips_for_mode(mode);

    let test_dir = match find_test_videos_dir() {
        Some(d) => d,
        None => {
            return TestSuiteResult {
                hwaccel: hwaccel_str,
                mode: format!("{:?}", mode).to_lowercase(),
                results: vec![],
                summary: TestSummary {
                    total: 0,
                    passed: 0,
                    failed: 0,
                    skipped: clips.len() as u32,
                    duration_secs: 0.0,
                },
            };
        }
    };

    // Determine which output codecs to test
    let output_codecs: Vec<Codec> = match mode {
        TestMode::Quick => vec![Codec::H265],
        TestMode::Full => {
            let mut codecs = vec![Codec::H264, Codec::H265];
            // Add AV1 if encoder is available for detected hwaccel
            if hwaccel.av1_encoder().is_some() {
                codecs.push(Codec::AV1);
            }
            codecs
        }
    };

    let mut results = Vec::new();

    for clip in &clips {
        let clip_path = test_dir.join(clip.filename);
        if !clip_path.exists() {
            results.push(TestResult {
                clip_name: clip.name.to_string(),
                output_codec: "-".into(),
                hwaccel: hwaccel_str.clone(),
                passed: false,
                checks: vec![],
                encode_time_secs: 0.0,
                speed_ratio: 0.0,
                error: Some(format!("Fixture not found: {}", clip.filename)),
            });
            continue;
        }

        let clip_url = format!("file://{}", clip_path.display());

        // Get source duration for speed ratio
        let source_duration = VideoMetadata::extract(&clip_url, &config.ffprobe_path)
            .await
            .ok()
            .and_then(|m| m.duration_secs())
            .unwrap_or(5.0);

        for &codec in &output_codecs {
            let result = run_single_test(
                &processor,
                &config,
                clip,
                &clip_url,
                codec,
                source_duration,
                &hwaccel_str,
            )
            .await;
            results.push(result);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count() as u32;
    let failed = results.iter().filter(|r| !r.passed && r.error.is_none()).count() as u32;
    let errored = results.iter().filter(|r| r.error.is_some() && !r.passed).count() as u32;

    TestSuiteResult {
        hwaccel: hwaccel_str,
        mode: format!("{:?}", mode).to_lowercase(),
        results: results.clone(),
        summary: TestSummary {
            total: results.len() as u32,
            passed,
            failed: failed + errored,
            skipped: 0,
            duration_secs: suite_start.elapsed().as_secs_f64(),
        },
    }
}

async fn run_single_test(
    processor: &VideoProcessor,
    config: &Config,
    clip: &TestClip,
    clip_url: &str,
    output_codec: Codec,
    source_duration: f64,
    hwaccel_str: &str,
) -> TestResult {
    info!(
        clip = clip.name,
        codec = output_codec.as_str(),
        "Running self-test"
    );

    let start = Instant::now();

    let encode_result = processor
        .transform_mp4(
            clip_url,
            Resolution::R720p,
            Some(28), // fast CRF for test
            output_codec,
            Some(clip.expected_codec),
            None,
            None,
        )
        .await;

    let encode_time = start.elapsed().as_secs_f64();

    match encode_result {
        Ok(result) => {
            let mut checks = vec![check_output_exists(&result.output_path)];

            // Validate with ffprobe
            match VideoMetadata::extract(
                result.output_path.to_str().unwrap_or(""),
                &config.ffprobe_path,
            )
            .await
            {
                Ok(meta) => {
                    checks.push(check_resolution(&meta, clip.expected_height));
                    checks.push(check_codec(&meta, output_codec.as_str()));
                    checks.push(check_duration(&meta, source_duration, 2.0));
                    checks.push(check_audio(&meta, clip.has_audio));
                }
                Err(e) => {
                    checks.push(Check {
                        name: "ffprobe".into(),
                        passed: false,
                        detail: format!("ffprobe failed: {}", e),
                    });
                }
            }

            let all_passed = checks.iter().all(|c| c.passed);
            let speed_ratio = if encode_time > 0.0 {
                source_duration / encode_time
            } else {
                0.0
            };

            result.cleanup().await;

            TestResult {
                clip_name: clip.name.to_string(),
                output_codec: output_codec.as_str().to_string(),
                hwaccel: hwaccel_str.to_string(),
                passed: all_passed,
                checks,
                encode_time_secs: encode_time,
                speed_ratio,
                error: None,
            }
        }
        Err(e) => {
            error!(clip = clip.name, error = %e, "Self-test encode failed");
            TestResult {
                clip_name: clip.name.to_string(),
                output_codec: output_codec.as_str().to_string(),
                hwaccel: hwaccel_str.to_string(),
                passed: false,
                checks: vec![],
                encode_time_secs: encode_time,
                speed_ratio: 0.0,
                error: Some(format!("Encode failed: {}", e)),
            }
        }
    }
}
```

**Step 1:** Write the file as above.

**Step 2:** Check that `hwaccel.av1_encoder()` exists — if not, use a simpler availability check. Look at `src/video/hwaccel.rs` for the method name.

**Step 3: Verify it compiles**

```bash
cargo check
```

**Step 4: Commit**

```bash
git add src/selftest/runner.rs
git commit -m "feat: add selftest runner with multi-clip support"
```

---

### Task 5: Update admin command types

**Files:**
- Modify: `src/admin/commands.rs`

**Changes:**

1. Update `AdminCommand::SelfTest` to accept a mode:
```rust
SelfTest {
    #[serde(default = "default_selftest_mode")]
    mode: String,
},
```

Add helper:
```rust
fn default_selftest_mode() -> String {
    "quick".to_string()
}
```

2. Update `AdminRequest::to_command()` for `"self_test"`:
```rust
"self_test" => {
    let mode = self.params.get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("quick")
        .to_string();
    Ok(AdminCommand::SelfTest { mode })
}
```

3. Replace `SelfTestResponse` with `SelfTestSuiteResponse`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfTestSuiteResponse {
    pub hwaccel: String,
    pub mode: String,
    pub results: Vec<SelfTestResultEntry>,
    pub summary: SelfTestSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfTestResultEntry {
    pub clip_name: String,
    pub output_codec: String,
    pub hwaccel: String,
    pub passed: bool,
    pub checks: Vec<SelfTestCheck>,
    pub encode_time_secs: f64,
    pub speed_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfTestCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfTestSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_secs: f64,
}
```

4. Update `ResponseData` enum — replace `SelfTest(SelfTestResponse)` with `SelfTest(SelfTestSuiteResponse)`.

5. Remove the old `SelfTestResponse` struct.

**Step 1:** Make all changes above.

**Step 2: Verify it compiles (expect handler.rs errors — that's next)**

```bash
cargo check 2>&1 | head -20
# Expect errors in handler.rs only
```

**Step 3: Commit**

```bash
git add src/admin/commands.rs
git commit -m "feat: update admin command types for multi-clip self-test"
```

---

### Task 6: Update admin handler to use new test runner

**Files:**
- Modify: `src/admin/handler.rs`

**Changes:**

1. Replace the `handle_self_test` method:
```rust
async fn handle_self_test(&self, mode_str: &str) -> AdminResponse {
    info!(mode = mode_str, "Starting self-test suite");

    let mode = crate::selftest::TestMode::from_str(mode_str);
    let suite_result = crate::selftest::runner::run_test_suite(
        self.config.clone(),
        mode,
    )
    .await;

    // Convert runner types to command types
    let results: Vec<SelfTestResultEntry> = suite_result
        .results
        .into_iter()
        .map(|r| SelfTestResultEntry {
            clip_name: r.clip_name,
            output_codec: r.output_codec,
            hwaccel: r.hwaccel,
            passed: r.passed,
            checks: r.checks.into_iter().map(|c| SelfTestCheck {
                name: c.name,
                passed: c.passed,
                detail: c.detail,
            }).collect(),
            encode_time_secs: r.encode_time_secs,
            speed_ratio: r.speed_ratio,
            error: r.error,
        })
        .collect();

    let response = SelfTestSuiteResponse {
        hwaccel: suite_result.hwaccel,
        mode: suite_result.mode,
        results,
        summary: SelfTestSummary {
            total: suite_result.summary.total,
            passed: suite_result.summary.passed,
            failed: suite_result.summary.failed,
            skipped: suite_result.summary.skipped,
            duration_secs: suite_result.summary.duration_secs,
        },
    };

    AdminResponse::ok_with_data(ResponseData::SelfTest(response))
}
```

2. Update the dispatch in `handle()`:
```rust
AdminCommand::SelfTest { mode } => self.handle_self_test(&mode).await,
```

3. Remove the `TEST_VIDEO_URL` constant and old imports no longer needed (`Instant`, etc. if they were only used by old self_test — check carefully).

**Step 1:** Make all changes.

**Step 2: Verify it compiles**

```bash
cargo check
```

**Step 3: Run existing tests**

```bash
cargo test
```

**Step 4: Commit**

```bash
git add src/admin/handler.rs
git commit -m "feat: wire admin handler to multi-clip selftest runner"
```

---

### Task 7: Update frontend admin types

**Files:**
- Modify: `frontend/src/nostr/admin.ts`

Find the `SelfTestResult` type and replace/extend it with the suite types:

```typescript
export interface SelfTestCheck {
  name: string;
  passed: boolean;
  detail: string;
}

export interface SelfTestResultEntry {
  clip_name: string;
  output_codec: string;
  hwaccel: string;
  passed: boolean;
  checks: SelfTestCheck[];
  encode_time_secs: number;
  speed_ratio: number;
  error?: string;
}

export interface SelfTestSummary {
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  duration_secs: number;
}

export interface SelfTestSuiteResult {
  hwaccel: string;
  mode: string;
  results: SelfTestResultEntry[];
  summary: SelfTestSummary;
}
```

Update `sendAdminCommand` call site for self_test to pass `{ mode }` param.

**Step 1:** Make changes.

**Step 2: Verify TypeScript compiles**

```bash
cd frontend && npx tsc --noEmit
```

**Step 3: Commit**

```bash
git add frontend/src/nostr/admin.ts
git commit -m "feat: update frontend admin types for selftest suite"
```

---

### Task 8: Update SelfTest.tsx component

**Files:**
- Modify: `frontend/src/components/SelfTest.tsx`
- Modify: `frontend/src/components/SelfTest.css` (if needed for new table styles)

**Changes:**

1. Add mode toggle (Quick/Full) above the Run Test button
2. Replace single-result display with results table:
   - Columns: Clip, Output Codec, Status (pass/fail), Speed, Encode Time
   - Expandable rows showing individual checks
3. Add summary bar: X/Y passed, total time, hwaccel

The response detection logic changes — look for `"summary"` and `"results"` fields instead of `"success"`.

Pass mode to command: `sendAdminCommand(signer, dvmPubkey, "self_test", { mode }, RELAYS)`

**Step 1:** Implement the component changes.

**Step 2: Verify TypeScript**

```bash
cd frontend && npx tsc --noEmit
```

**Step 3: Visual check**

```bash
cd frontend && npm run dev
# Open browser, verify the SelfTest panel renders mode toggle + table
```

**Step 4: Commit**

```bash
git add frontend/src/components/SelfTest.tsx frontend/src/components/SelfTest.css
git commit -m "feat: update SelfTest UI with mode toggle and results table"
```

---

### Task 9: Update integration tests

**Files:**
- Modify: `tests/remote_config_integration.rs` (if it references old SelfTest command shape)

Check if any integration test constructs an `AdminCommand::SelfTest` — if so, update to `AdminCommand::SelfTest { mode: "quick".to_string() }`.

**Step 1:** Search and update.

```bash
cargo test
```

**Step 2: Commit if changed**

```bash
git add tests/
git commit -m "fix: update integration tests for new SelfTest command shape"
```

---

### Task 10: Final verification and cleanup

**Step 1: Full build**

```bash
cargo build
```

**Step 2: All Rust tests**

```bash
cargo test
```

**Step 3: Frontend check**

```bash
cd frontend && npx tsc --noEmit && npm run lint
```

**Step 4: Clippy**

```bash
cargo clippy
```

**Step 5: Fix any issues, commit**

```bash
git add -A
git commit -m "chore: cleanup selftest suite implementation"
```
