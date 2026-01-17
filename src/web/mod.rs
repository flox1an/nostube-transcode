mod assets;

use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::process::Command as TokioCommand;
use tracing::{error, info};

use crate::dvm::events::{Codec, Resolution};
use crate::video::hwaccel::HwAccel;
use crate::video::{VideoMetadata, VideoProcessor};
use crate::Config;
use assets::Assets;

/// Test video URL for self-test
const TEST_VIDEO_URL: &str = "https://almond.slidestr.net/ecf8f3a25b4a6109c5aa6ea90ee97f8cafec09f99a2f71f0e6253c3bdf26ccea";

pub async fn run_server(config: Arc<Config>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/system-info", get(system_info_handler))
        .route("/selftest", get(selftest_handler))
        .route("/*path", get(static_handler))
        .with_state(config.clone());

    let addr = format!("0.0.0.0:{}", config.http_port);
    let listener = TcpListener::bind(&addr).await?;

    info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler() -> impl IntoResponse {
    serve_file("index.html")
}

/// Hardware encoder info
#[derive(Serialize)]
struct HwEncoderInfo {
    /// Encoder name (e.g., "NVIDIA NVENC")
    name: String,
    /// Whether this is the currently selected encoder
    selected: bool,
    /// Supported codecs
    codecs: Vec<String>,
}

/// GPU information
#[derive(Serialize)]
struct GpuInfo {
    /// GPU name/model
    name: String,
    /// GPU vendor
    vendor: String,
    /// Additional details (driver version, VRAM, etc.)
    details: Option<String>,
}

/// Disk space information
#[derive(Serialize)]
struct DiskInfo {
    /// Path being checked
    path: String,
    /// Free space in bytes
    free_bytes: u64,
    /// Total space in bytes
    total_bytes: u64,
    /// Free space as percentage
    free_percent: f64,
}

/// FFmpeg information
#[derive(Serialize)]
struct FfmpegInfo {
    /// Path to FFmpeg binary
    path: String,
    /// FFmpeg version string
    version: Option<String>,
    /// Path to FFprobe binary
    ffprobe_path: String,
}

/// System information response
#[derive(Serialize)]
struct SystemInfo {
    /// Platform (macos, linux, windows)
    platform: String,
    /// Architecture (x86_64, aarch64, etc.)
    arch: String,
    /// Available hardware encoders
    hw_encoders: Vec<HwEncoderInfo>,
    /// GPU information (if available)
    gpu: Option<GpuInfo>,
    /// Disk space information
    disk: DiskInfo,
    /// FFmpeg information
    ffmpeg: FfmpegInfo,
    /// Temp directory path
    temp_dir: String,
}

/// Get FFmpeg version
async fn get_ffmpeg_version(ffmpeg_path: &std::path::Path) -> Option<String> {
    let output = TokioCommand::new(ffmpeg_path)
        .arg("-version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // First line contains version, e.g., "ffmpeg version 6.0 Copyright..."
        stdout.lines().next().map(|s| s.to_string())
    } else {
        None
    }
}

/// Get GPU information
async fn get_gpu_info() -> Option<GpuInfo> {
    #[cfg(target_os = "macos")]
    {
        // Use system_profiler on macOS
        let output = TokioCommand::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
            .await
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse JSON to get GPU name
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(displays) = json.get("SPDisplaysDataType").and_then(|v| v.as_array()) {
                    if let Some(first) = displays.first() {
                        let name = first
                            .get("sppci_model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let vendor = first
                            .get("spdisplays_vendor")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Apple")
                            .to_string();
                        let vram = first
                            .get("spdisplays_vram")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        return Some(GpuInfo {
                            name,
                            vendor,
                            details: vram,
                        });
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    {
        // Try nvidia-smi first
        if let Ok(output) = TokioCommand::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total,driver_version", "--format=csv,noheader"])
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split(',').map(|s| s.trim()).collect();
                if !parts.is_empty() {
                    return Some(GpuInfo {
                        name: parts.first().unwrap_or(&"Unknown").to_string(),
                        vendor: "NVIDIA".to_string(),
                        details: if parts.len() >= 3 {
                            Some(format!("VRAM: {}, Driver: {}", parts[1], parts[2]))
                        } else {
                            None
                        },
                    });
                }
            }
        }

        // Fallback to lspci
        if let Ok(output) = TokioCommand::new("lspci")
            .args(["-nn"])
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("VGA") || line.contains("3D controller") {
                        let vendor = if line.contains("NVIDIA") {
                            "NVIDIA"
                        } else if line.contains("Intel") {
                            "Intel"
                        } else if line.contains("AMD") || line.contains("ATI") {
                            "AMD"
                        } else {
                            "Unknown"
                        };
                        return Some(GpuInfo {
                            name: line.to_string(),
                            vendor: vendor.to_string(),
                            details: None,
                        });
                    }
                }
            }
        }

        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Get disk space info for a path
fn get_disk_info(path: &std::path::Path) -> DiskInfo {
    use std::ffi::CString;

    let path_str = path.to_string_lossy().to_string();

    #[cfg(unix)]
    {
        // Handle potential null bytes in path (unlikely but possible)
        let c_path = match CString::new(path_str.as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(path = %path_str, "Path contains null bytes, cannot get disk info");
                return DiskInfo {
                    path: path_str,
                    free_bytes: 0,
                    total_bytes: 0,
                    free_percent: 0.0,
                };
            }
        };
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };

        if result == 0 {
            let free_bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
            let total_bytes = stat.f_blocks as u64 * stat.f_frsize as u64;
            let free_percent = if total_bytes > 0 {
                (free_bytes as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            return DiskInfo {
                path: path_str,
                free_bytes,
                total_bytes,
                free_percent,
            };
        }
    }

    // Fallback for non-unix or on error
    DiskInfo {
        path: path_str,
        free_bytes: 0,
        total_bytes: 0,
        free_percent: 0.0,
    }
}

async fn system_info_handler(State(config): State<Arc<Config>>) -> impl IntoResponse {
    info!("Getting system info");

    // Detect hardware encoders
    let selected_hwaccel = HwAccel::detect();
    let available_hwaccels = HwAccel::detect_all();

    let hw_encoders: Vec<HwEncoderInfo> = available_hwaccels
        .into_iter()
        .map(|hw| {
            let codecs = vec!["H.264".to_string(), "H.265 (HEVC)".to_string()];
            HwEncoderInfo {
                name: hw.name().to_string(),
                selected: hw == selected_hwaccel,
                codecs,
            }
        })
        .collect();

    // Get GPU info
    let gpu = get_gpu_info().await;

    // Get disk info for temp directory
    let disk = get_disk_info(&config.temp_dir);

    // Get FFmpeg info
    let ffmpeg_version = get_ffmpeg_version(&config.ffmpeg_path).await;
    let ffmpeg = FfmpegInfo {
        path: config.ffmpeg_path.to_string_lossy().to_string(),
        version: ffmpeg_version,
        ffprobe_path: config.ffprobe_path.to_string_lossy().to_string(),
    };

    Json(SystemInfo {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        hw_encoders,
        gpu,
        disk,
        ffmpeg,
        temp_dir: config.temp_dir.to_string_lossy().to_string(),
    })
}

/// Response from the self-test endpoint
#[derive(Serialize)]
struct SelfTestResult {
    /// Whether the test passed
    success: bool,
    /// Test video URL used
    test_video_url: String,
    /// Video duration in seconds
    video_duration_secs: f64,
    /// Encoding time in seconds
    encode_time_secs: f64,
    /// Speed ratio (e.g., 2.5 means encoding was 2.5x realtime)
    speed_ratio: f64,
    /// Human-readable speed description
    speed_description: String,
    /// Hardware acceleration method used
    hwaccel: String,
    /// Output resolution
    resolution: String,
    /// Output file size in bytes
    output_size_bytes: u64,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn selftest_handler(State(config): State<Arc<Config>>) -> impl IntoResponse {
    info!("Starting self-test with video: {}", TEST_VIDEO_URL);

    let resolution = Resolution::R720p;

    // Get video metadata to determine duration
    let metadata = match VideoMetadata::extract(TEST_VIDEO_URL, &config.ffprobe_path).await {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to extract metadata: {}", e);
            return Json(SelfTestResult {
                success: false,
                test_video_url: TEST_VIDEO_URL.to_string(),
                video_duration_secs: 0.0,
                encode_time_secs: 0.0,
                speed_ratio: 0.0,
                speed_description: "N/A".to_string(),
                hwaccel: "unknown".to_string(),
                resolution: resolution.as_str().to_string(),
                output_size_bytes: 0,
                error: Some(format!("Failed to extract metadata: {}", e)),
            });
        }
    };

    let video_duration = metadata.duration_secs().unwrap_or(0.0);
    info!(duration_secs = video_duration, "Video metadata extracted");

    // Create video processor
    let processor = VideoProcessor::new(config.clone());
    let hwaccel = processor.hwaccel();

    // Time the encoding
    let start = Instant::now();

    let result = match processor.transform_mp4(TEST_VIDEO_URL, resolution, Some(23), Codec::default()).await {
        Ok(result) => result,
        Err(e) => {
            error!("Self-test encoding failed: {}", e);
            return Json(SelfTestResult {
                success: false,
                test_video_url: TEST_VIDEO_URL.to_string(),
                video_duration_secs: video_duration,
                encode_time_secs: start.elapsed().as_secs_f64(),
                speed_ratio: 0.0,
                speed_description: "N/A".to_string(),
                hwaccel: hwaccel.to_string(),
                resolution: resolution.as_str().to_string(),
                output_size_bytes: 0,
                error: Some(format!("Encoding failed: {}", e)),
            });
        }
    };

    let encode_time = start.elapsed().as_secs_f64();
    let speed_ratio = if encode_time > 0.0 {
        video_duration / encode_time
    } else {
        0.0
    };

    let speed_description = if speed_ratio >= 1.0 {
        format!("{:.1}x realtime (faster than realtime)", speed_ratio)
    } else if speed_ratio > 0.0 {
        format!("{:.1}x realtime (slower than realtime)", speed_ratio)
    } else {
        "N/A".to_string()
    };

    // Get output file size
    let output_size_bytes = tokio::fs::metadata(&result.output_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!(
        encode_time_secs = encode_time,
        speed_ratio = speed_ratio,
        output_size_bytes = output_size_bytes,
        hwaccel = %hwaccel,
        "Self-test complete"
    );

    // Cleanup temp files
    result.cleanup().await;

    Json(SelfTestResult {
        success: true,
        test_video_url: TEST_VIDEO_URL.to_string(),
        video_duration_secs: video_duration,
        encode_time_secs: encode_time,
        speed_ratio,
        speed_description,
        hwaccel: hwaccel.to_string(),
        resolution: resolution.as_str().to_string(),
        output_size_bytes,
        error: None,
    })
}

async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    serve_file(&path)
}

fn serve_file(path: &str) -> Response<Body> {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap_or_else(|e| {
                    error!(error = %e, "Failed to build response");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .expect("fallback response must build")
                })
        }
        None => {
            // For SPA routing: serve index.html for unknown paths
            match Assets::get("index.html") {
                Some(content) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data.to_vec()))
                    .unwrap_or_else(|e| {
                        error!(error = %e, "Failed to build index response");
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from("Internal Server Error"))
                            .expect("fallback response must build")
                    }),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .expect("not found response must build"),
            }
        }
    }
}
