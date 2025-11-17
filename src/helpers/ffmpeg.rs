use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

use crate::helpers::dvm::{OutputFormat, Resolution};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub duration: f64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct TransformConfig {
    pub output_format: OutputFormat,
    pub resolutions: Vec<Resolution>,
    pub hls_time: u32,
    pub hls_list_size: u32,
    pub segment_type: String,
    pub video_codec: String,
    pub quality: u32,
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::Hls,
            resolutions: vec![Resolution::R480p, Resolution::R720p, Resolution::R1080p],
            hls_time: 6,
            hls_list_size: 0,
            segment_type: "fmp4".to_string(),
            video_codec: "libx265".to_string(),
            quality: 28,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FfprobeOutput {
    format: FfprobeFormat,
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FfprobeStream {
    codec_type: String,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug)]
pub struct ProcessedVideo {
    pub output_format: OutputFormat,
    pub master_playlist: Option<PathBuf>,
    pub stream_playlists: Vec<PathBuf>,
    pub segments: Vec<PathBuf>,
    pub video_files: Vec<PathBuf>,
    pub metadata: VideoMetadata,
}

/// Extract video metadata using ffprobe
pub async fn get_video_metadata(url: &str) -> Result<VideoMetadata, Box<dyn std::error::Error>> {
    log::info!("Extracting metadata from: {}", url);

    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            url,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr).into());
    }

    let probe: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    let duration = probe
        .format
        .duration
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    let size = probe
        .format
        .size
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let video_stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .ok_or("No video stream found")?;

    let width = video_stream.width.unwrap_or(0);
    let height = video_stream.height.unwrap_or(0);

    Ok(VideoMetadata {
        duration,
        size,
        width,
        height,
    })
}

/// Process video to the specified format
pub async fn process_video(
    url: &str,
    output_dir: &Path,
    config: &TransformConfig,
) -> Result<ProcessedVideo, Box<dyn std::error::Error>> {
    log::info!("Processing video: {} (format: {:?})", url, config.output_format);

    // Create output directory
    fs::create_dir_all(output_dir).await?;

    // Get metadata first
    let metadata = get_video_metadata(url).await?;
    log::info!(
        "Video metadata: {}x{}, {:.2}s, {} bytes",
        metadata.width,
        metadata.height,
        metadata.duration,
        metadata.size
    );

    match config.output_format {
        OutputFormat::Hls => process_video_hls(url, output_dir, config, metadata).await,
        OutputFormat::Mp4 => process_video_mp4(url, output_dir, config, metadata).await,
    }
}

/// Process video to HLS format
async fn process_video_hls(
    url: &str,
    output_dir: &Path,
    config: &TransformConfig,
    metadata: VideoMetadata,
) -> Result<ProcessedVideo, Box<dyn std::error::Error>> {
    log::info!("Processing to HLS format with {} resolutions", config.resolutions.len());

    let output_pattern = output_dir.join("stream_%v.m3u8");
    let master_name = output_dir.join("master.m3u8");

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(url);

    // Build filter complex for multiple resolutions
    let mut filter_parts = Vec::new();
    let mut var_stream_map_parts = Vec::new();

    let num_resolutions = config.resolutions.len();

    // Build split filter if needed
    if num_resolutions > 1 {
        let split_outputs: Vec<String> = (0..num_resolutions)
            .map(|i| format!("v{}", i))
            .collect();
        filter_parts.push(format!("[0:v]split={}[{}]", num_resolutions, split_outputs.join("][")));
    } else if num_resolutions == 1 {
        filter_parts.push("[0:v]copy[v0]".to_string());
    }

    for (idx, resolution) in config.resolutions.iter().enumerate() {
        let input_label = if num_resolutions > 1 {
            format!("v{}", idx)
        } else {
            "v0".to_string()
        };
        let output_label = format!("v{}out", idx);

        // Add scale filter
        filter_parts.push(format!(
            "[{}]scale={}:{}[{}]",
            input_label,
            resolution.width(),
            resolution.height(),
            output_label
        ));

        // Map video stream
        cmd.arg("-map").arg(format!("[{}]", output_label));
        cmd.arg(format!("-c:v:{}", idx)).arg(&config.video_codec);
        cmd.arg(format!("-crf:{}", idx)).arg(config.quality.to_string());

        // Map audio stream
        cmd.arg("-map").arg("0:a");
        cmd.arg(format!("-c:a:{}", idx)).arg("aac");
        cmd.arg(format!("-b:a:{}", idx)).arg("128k");

        // Add to variant stream map
        var_stream_map_parts.push(format!("v:{},a:{}", idx, idx));
    }

    // Add filter complex
    if !filter_parts.is_empty() {
        cmd.arg("-filter_complex").arg(filter_parts.join("; "));
    }

    // Add HLS output options
    cmd.arg("-f")
        .arg("hls")
        .arg("-var_stream_map")
        .arg(var_stream_map_parts.join(" "))
        .arg("-hls_time")
        .arg(config.hls_time.to_string())
        .arg("-hls_list_size")
        .arg(config.hls_list_size.to_string())
        .arg("-hls_segment_type")
        .arg(&config.segment_type)
        .arg("-master_pl_name")
        .arg("master.m3u8")
        .arg(&output_pattern);

    log::info!("Running FFmpeg command: {:?}", cmd);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg failed: {}", stderr).into());
    }

    log::info!("FFmpeg HLS processing completed");

    // Collect generated files
    let mut stream_playlists = Vec::new();
    let mut segments = Vec::new();

    let mut entries = fs::read_dir(output_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                match ext.to_str() {
                    Some("m3u8") => {
                        if path.file_name().unwrap() != "master.m3u8" {
                            stream_playlists.push(path);
                        }
                    }
                    Some("m4s") | Some("ts") => {
                        segments.push(path);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(ProcessedVideo {
        output_format: OutputFormat::Hls,
        master_playlist: Some(master_name),
        stream_playlists,
        segments,
        video_files: Vec::new(),
        metadata,
    })
}

/// Process video to MP4 single-file format
async fn process_video_mp4(
    url: &str,
    output_dir: &Path,
    config: &TransformConfig,
    metadata: VideoMetadata,
) -> Result<ProcessedVideo, Box<dyn std::error::Error>> {
    log::info!("Processing to MP4 format with {} resolutions", config.resolutions.len());

    let mut video_files = Vec::new();

    // Process each resolution separately
    for resolution in &config.resolutions {
        let output_file = output_dir.join(format!("video_{}.mp4", resolution.as_str()));

        log::info!("Processing {} resolution to {}", resolution.as_str(), output_file.display());

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-i")
            .arg(url)
            .arg("-vf")
            .arg(format!("scale={}:{}", resolution.width(), resolution.height()))
            .arg("-c:v")
            .arg(&config.video_codec)
            .arg("-crf")
            .arg(config.quality.to_string())
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("128k")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&output_file);

        log::debug!("Running FFmpeg command: {:?}", cmd);

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("FFmpeg failed for {}: {}", resolution.as_str(), stderr).into());
        }

        log::info!("Completed {} resolution", resolution.as_str());
        video_files.push(output_file);
    }

    log::info!("FFmpeg MP4 processing completed");

    Ok(ProcessedVideo {
        output_format: OutputFormat::Mp4,
        master_playlist: None,
        stream_playlists: Vec::new(),
        segments: Vec::new(),
        video_files,
        metadata,
    })
}

/// Calculate SHA-256 hash of a file
pub async fn calculate_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let contents = fs::read(file_path).await?;
    let hash = Sha256::digest(&contents);
    Ok(format!("{:x}", hash))
}

/// Rename files to content-addressable names and update playlists (for HLS)
pub async fn make_content_addressable(
    processed: &ProcessedVideo,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    log::info!("Making files content-addressable");

    let mut hash_map: HashMap<String, String> = HashMap::new();

    match processed.output_format {
        OutputFormat::Hls => make_hls_content_addressable(processed, &mut hash_map).await?,
        OutputFormat::Mp4 => make_mp4_content_addressable(processed, &mut hash_map).await?,
    }

    log::info!("Content-addressable naming completed");
    Ok(hash_map)
}

async fn make_hls_content_addressable(
    processed: &ProcessedVideo,
    hash_map: &mut HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Hash all segments and rename them
    for segment_path in &processed.segments {
        let hash = calculate_sha256(segment_path).await?;
        let extension = segment_path.extension().unwrap_or_default();
        let new_name = format!("{}.{}", hash, extension.to_string_lossy());
        let new_path = segment_path.with_file_name(&new_name);

        let old_name = segment_path.file_name().unwrap().to_string_lossy().to_string();
        hash_map.insert(old_name, new_name.clone());

        fs::rename(segment_path, &new_path).await?;
        log::debug!("Renamed segment: {} -> {}", segment_path.display(), new_path.display());
    }

    // Step 2: Update stream playlists and rename them
    let mut stream_hash_map: HashMap<String, String> = HashMap::new();

    for playlist_path in &processed.stream_playlists {
        update_playlist(playlist_path, hash_map).await?;

        let hash = calculate_sha256(playlist_path).await?;
        let new_name = format!("{}.m3u8", hash);
        let new_path = playlist_path.with_file_name(&new_name);

        let old_name = playlist_path.file_name().unwrap().to_string_lossy().to_string();
        stream_hash_map.insert(old_name, new_name.clone());

        fs::rename(playlist_path, &new_path).await?;
        log::debug!("Renamed playlist: {} -> {}", playlist_path.display(), new_path.display());
    }

    // Step 3: Update master playlist
    if let Some(master_playlist) = &processed.master_playlist {
        update_playlist(master_playlist, &stream_hash_map).await?;

        // Calculate master playlist hash (but don't rename it - keep it as master.m3u8)
        let master_hash = calculate_sha256(master_playlist).await?;
        hash_map.insert("master.m3u8".to_string(), master_hash);
    }

    Ok(())
}

async fn make_mp4_content_addressable(
    processed: &ProcessedVideo,
    hash_map: &mut HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // For MP4 files, we just hash them and rename
    for video_path in &processed.video_files {
        let hash = calculate_sha256(video_path).await?;
        let new_name = format!("{}.mp4", hash);
        let new_path = video_path.with_file_name(&new_name);

        let old_name = video_path.file_name().unwrap().to_string_lossy().to_string();
        hash_map.insert(old_name, hash.clone());

        fs::rename(video_path, &new_path).await?;
        log::debug!("Renamed video: {} -> {}", video_path.display(), new_path.display());
    }

    Ok(())
}

/// Update playlist file with new filenames
async fn update_playlist(
    playlist_path: &Path,
    replacements: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(playlist_path).await?;
    let mut updated_content = content.clone();

    // Update URI references in #EXT-X-MAP
    let uri_regex = Regex::new(r#"URI="([^"]+)""#)?;
    for (old_name, new_name) in replacements {
        updated_content = updated_content.replace(old_name, new_name);
    }

    // Also handle URI= patterns
    updated_content = uri_regex
        .replace_all(&updated_content, |caps: &regex::Captures| {
            let uri = &caps[1];
            if let Some((old_name, new_name)) = replacements.iter().find(|(old, _)| uri.contains(*old)) {
                format!(r#"URI="{}""#, uri.replace(old_name, new_name))
            } else {
                caps[0].to_string()
            }
        })
        .to_string();

    fs::write(playlist_path, updated_content).await?;
    Ok(())
}
