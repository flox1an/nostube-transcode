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
use tracing::{error, info};

use crate::dvm::events::Resolution;
use crate::video::{VideoMetadata, VideoProcessor};
use crate::Config;
use assets::Assets;

/// Test video URL for self-test
const TEST_VIDEO_URL: &str = "https://almond.slidestr.net/ecf8f3a25b4a6109c5aa6ea90ee97f8cafec09f99a2f71f0e6253c3bdf26ccea";

pub async fn run_server(config: Arc<Config>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
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

    let result = match processor.transform_mp4(TEST_VIDEO_URL, resolution, Some(23)).await {
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
                .unwrap()
        }
        None => {
            // For SPA routing: serve index.html for unknown paths
            match Assets::get("index.html") {
                Some(content) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data.to_vec()))
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap(),
            }
        }
    }
}
