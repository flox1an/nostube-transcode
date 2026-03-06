use crate::config::Config;
use crate::dvm::events::{Codec, Resolution};
use crate::selftest::validate::*;
use crate::selftest::{clips_for_mode, TestClip, TestMode};
use crate::video::{VideoMetadata, VideoProcessor};
use serde::Serialize;
use std::path::PathBuf;
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

/// Look for a `test-videos/` directory relative to the current working
/// directory first, then next to the running binary.
fn find_test_videos_dir() -> Option<PathBuf> {
    // Try relative to cwd
    let cwd_path = PathBuf::from("./test-videos");
    if cwd_path.is_dir() {
        return Some(cwd_path.canonicalize().unwrap_or(cwd_path));
    }

    // Try next to the binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let exe_path = exe_dir.join("test-videos");
            if exe_path.is_dir() {
                return Some(exe_path);
            }
        }
    }

    None
}

/// Run the full self-test suite.
pub async fn run_test_suite(config: Arc<Config>, mode: TestMode) -> TestSuiteResult {
    let suite_start = Instant::now();
    let processor = VideoProcessor::new(config.clone());
    let hwaccel = processor.hwaccel();
    let hwaccel_str = hwaccel.to_string();

    let clips = clips_for_mode(mode);

    let mode_str = match mode {
        TestMode::Quick => "quick",
        TestMode::Full => "full",
    };

    // Find test videos directory
    let test_dir = match find_test_videos_dir() {
        Some(dir) => {
            info!(path = %dir.display(), "Found test-videos directory");
            dir
        }
        None => {
            info!("test-videos directory not found, skipping all clips");
            let results: Vec<TestResult> = clips
                .iter()
                .map(|clip| TestResult {
                    clip_name: clip.name.to_string(),
                    output_codec: String::new(),
                    hwaccel: hwaccel_str.clone(),
                    passed: false,
                    checks: Vec::new(),
                    encode_time_secs: 0.0,
                    speed_ratio: 0.0,
                    error: Some("test-videos directory not found".to_string()),
                })
                .collect();
            let skipped = results.len() as u32;
            return TestSuiteResult {
                hwaccel: hwaccel_str,
                mode: mode_str.to_string(),
                results,
                summary: TestSummary {
                    total: skipped,
                    passed: 0,
                    failed: 0,
                    skipped,
                    duration_secs: suite_start.elapsed().as_secs_f64(),
                },
            };
        }
    };

    // Determine output codecs based on mode
    let output_codecs: Vec<Codec> = match mode {
        TestMode::Quick => vec![Codec::H265],
        TestMode::Full => {
            let mut codecs = vec![Codec::H264, Codec::H265];
            let av1_encoder = hwaccel.video_encoder(Codec::AV1);
            if av1_encoder.contains("av1") {
                codecs.push(Codec::AV1);
            } else {
                info!(
                    encoder = av1_encoder,
                    "AV1 encoder not available (fell back to {}), skipping AV1 tests", av1_encoder
                );
            }
            codecs
        }
    };

    let mut results = Vec::new();

    for clip in &clips {
        let clip_path = test_dir.join(clip.filename);
        if !clip_path.exists() {
            info!(clip = clip.name, file = clip.filename, "Clip file not found, skipping");
            for codec in &output_codecs {
                results.push(TestResult {
                    clip_name: clip.name.to_string(),
                    output_codec: codec.as_str().to_string(),
                    hwaccel: hwaccel_str.clone(),
                    passed: false,
                    checks: Vec::new(),
                    encode_time_secs: 0.0,
                    speed_ratio: 0.0,
                    error: Some(format!("Clip file not found: {}", clip.filename)),
                });
            }
            continue;
        }

        let clip_url = clip_path.to_string_lossy().to_string();

        // Extract source duration for speed_ratio calculation
        let source_duration = match VideoMetadata::extract(&clip_url, &config.ffprobe_path).await {
            Ok(meta) => meta.duration_secs().unwrap_or(0.0),
            Err(e) => {
                error!(clip = clip.name, error = %e, "Failed to extract source metadata");
                0.0
            }
        };

        for codec in &output_codecs {
            let result = run_single_test(
                &processor,
                &config,
                clip,
                &clip_url,
                *codec,
                source_duration,
                &hwaccel_str,
            )
            .await;
            info!(
                clip = clip.name,
                codec = codec.as_str(),
                passed = result.passed,
                time = format!("{:.1}s", result.encode_time_secs),
                "Test completed"
            );
            results.push(result);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count() as u32;
    let failed = results.iter().filter(|r| !r.passed && r.error.is_none()).count() as u32;
    let skipped = results.iter().filter(|r| r.error.is_some() && !r.passed).count() as u32;
    let total = results.len() as u32;

    TestSuiteResult {
        hwaccel: hwaccel_str,
        mode: mode_str.to_string(),
        results,
        summary: TestSummary {
            total,
            passed,
            failed,
            skipped,
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
    let start = Instant::now();

    let source_codec_str = clip.expected_codec;

    let transform_result = processor
        .transform_mp4(
            clip_url,
            Resolution::R720p,
            Some(28),
            output_codec,
            Some(source_codec_str),
            None,
            None,
        )
        .await;

    let encode_time = start.elapsed().as_secs_f64();
    let speed_ratio = if source_duration > 0.0 {
        source_duration / encode_time
    } else {
        0.0
    };

    match transform_result {
        Ok(result) => {
            let output_path = result.output_path.clone();

            // Run validation checks
            let mut checks = Vec::new();

            // 1. Output exists
            checks.push(check_output_exists(&output_path));

            // 2-5. Probe output metadata for remaining checks
            match VideoMetadata::extract(
                &output_path.to_string_lossy(),
                &config.ffprobe_path,
            )
            .await
            {
                Ok(out_meta) => {
                    // 2. Resolution check (720p max height)
                    checks.push(check_resolution(&out_meta, 720));

                    // 3. Codec check
                    checks.push(check_codec(&out_meta, output_codec.as_str()));

                    // 4. Duration check (within 2.0s tolerance)
                    if source_duration > 0.0 {
                        checks.push(check_duration(&out_meta, source_duration, 2.0));
                    }

                    // 5. Audio check
                    checks.push(check_audio(&out_meta, clip.has_audio));
                }
                Err(e) => {
                    error!(error = %e, "Failed to extract output metadata");
                    checks.push(Check {
                        name: "metadata_extraction".to_string(),
                        passed: false,
                        detail: format!("Failed to extract output metadata: {}", e),
                    });
                }
            }

            let passed = checks.iter().all(|c| c.passed);

            // Cleanup temp files
            result.cleanup().await;

            TestResult {
                clip_name: clip.name.to_string(),
                output_codec: output_codec.as_str().to_string(),
                hwaccel: hwaccel_str.to_string(),
                passed,
                checks,
                encode_time_secs: encode_time,
                speed_ratio,
                error: None,
            }
        }
        Err(e) => {
            error!(
                clip = clip.name,
                codec = output_codec.as_str(),
                error = %e,
                "Transform failed"
            );
            TestResult {
                clip_name: clip.name.to_string(),
                output_codec: output_codec.as_str().to_string(),
                hwaccel: hwaccel_str.to_string(),
                passed: false,
                checks: Vec::new(),
                encode_time_secs: encode_time,
                speed_ratio,
                error: Some(format!("{}", e)),
            }
        }
    }
}
