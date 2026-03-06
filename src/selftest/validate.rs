use crate::video::VideoMetadata;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Verify file exists and is non-empty.
pub fn check_output_exists(path: &Path) -> Check {
    let name = "output_exists".to_string();
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > 0 => Check {
            name,
            passed: true,
            detail: format!("File exists ({} bytes): {}", meta.len(), path.display()),
        },
        Ok(_) => Check {
            name,
            passed: false,
            detail: format!("File exists but is empty: {}", path.display()),
        },
        Err(e) => Check {
            name,
            passed: false,
            detail: format!("File not found: {} ({})", path.display(), e),
        },
    }
}

/// Verify dimensions are non-zero, even numbers, and output height <= max_height.
pub fn check_resolution(metadata: &VideoMetadata, max_height: u32) -> Check {
    let name = "resolution".to_string();
    match metadata.resolution() {
        Some((width, height)) => {
            let mut issues = Vec::new();
            if width == 0 || height == 0 {
                issues.push(format!("dimensions must be non-zero (got {}x{})", width, height));
            }
            if width % 2 != 0 {
                issues.push(format!("width {} is not even", width));
            }
            if height % 2 != 0 {
                issues.push(format!("height {} is not even", height));
            }
            if height > max_height {
                issues.push(format!(
                    "height {} exceeds max_height {}",
                    height, max_height
                ));
            }
            if issues.is_empty() {
                Check {
                    name,
                    passed: true,
                    detail: format!("Resolution {}x{} (max height: {})", width, height, max_height),
                }
            } else {
                Check {
                    name,
                    passed: false,
                    detail: format!(
                        "Resolution {}x{}: {}",
                        width,
                        height,
                        issues.join("; ")
                    ),
                }
            }
        }
        None => Check {
            name,
            passed: false,
            detail: "No video stream or missing resolution info".to_string(),
        },
    }
}

/// Normalize a codec name for comparison.
fn normalize_codec(codec: &str) -> &str {
    match codec {
        "h265" | "hevc" => "hevc",
        "h264" => "h264",
        "av1" => "av1",
        other => other,
    }
}

/// Verify output codec matches expected. Normalizes h265/hevc and av1 variants.
pub fn check_codec(metadata: &VideoMetadata, expected: &str) -> Check {
    let name = "codec".to_string();
    match metadata.video_stream().and_then(|s| s.codec_name.as_deref()) {
        Some(actual) => {
            let actual_norm = normalize_codec(actual);
            let expected_norm = normalize_codec(expected);
            if actual_norm == expected_norm {
                Check {
                    name,
                    passed: true,
                    detail: format!("Codec matches: {} (normalized: {})", actual, actual_norm),
                }
            } else {
                Check {
                    name,
                    passed: false,
                    detail: format!(
                        "Codec mismatch: expected {} (normalized: {}), got {} (normalized: {})",
                        expected, expected_norm, actual, actual_norm
                    ),
                }
            }
        }
        None => Check {
            name,
            passed: false,
            detail: "No video stream or missing codec info".to_string(),
        },
    }
}

/// Verify duration is within tolerance of expected value.
pub fn check_duration(metadata: &VideoMetadata, expected_secs: f64, tolerance: f64) -> Check {
    let name = "duration".to_string();
    match metadata.duration_secs() {
        Some(actual) => {
            let diff = (actual - expected_secs).abs();
            if diff <= tolerance {
                Check {
                    name,
                    passed: true,
                    detail: format!(
                        "Duration {:.2}s within tolerance {:.2}s of expected {:.2}s",
                        actual, tolerance, expected_secs
                    ),
                }
            } else {
                Check {
                    name,
                    passed: false,
                    detail: format!(
                        "Duration {:.2}s differs from expected {:.2}s by {:.2}s (tolerance: {:.2}s)",
                        actual, expected_secs, diff, tolerance
                    ),
                }
            }
        }
        None => Check {
            name,
            passed: false,
            detail: "Could not determine duration from metadata".to_string(),
        },
    }
}

/// Verify audio presence matches expectation.
pub fn check_audio(metadata: &VideoMetadata, expected_has_audio: bool) -> Check {
    let name = "audio".to_string();
    let has_audio = metadata.audio_stream().is_some();
    if has_audio == expected_has_audio {
        Check {
            name,
            passed: true,
            detail: if has_audio {
                "Audio stream present as expected".to_string()
            } else {
                "No audio stream, as expected".to_string()
            },
        }
    } else {
        Check {
            name,
            passed: false,
            detail: if expected_has_audio {
                "Expected audio stream but none found".to_string()
            } else {
                "Found unexpected audio stream".to_string()
            },
        }
    }
}
