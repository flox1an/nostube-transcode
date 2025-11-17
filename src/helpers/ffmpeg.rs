use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub duration: f64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct ResolutionConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_bitrate: Option<String>,
    pub audio_bitrate: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub quality: Option<u32>,
    pub is_original: bool,
}

#[derive(Debug, Clone)]
pub struct TransformConfig {
    pub resolutions: HashMap<String, ResolutionConfig>,
    pub hls_time: u32,
    pub hls_list_size: u32,
    pub segment_type: String,
}

impl Default for TransformConfig {
    fn default() -> Self {
        let mut resolutions = HashMap::new();

        // 360p
        resolutions.insert(
            "360p".to_string(),
            ResolutionConfig {
                width: Some(640),
                height: Some(360),
                video_bitrate: None,
                audio_bitrate: Some("96k".to_string()),
                video_codec: Some("libx265".to_string()),
                audio_codec: Some("aac".to_string()),
                quality: Some(50),
                is_original: false,
            },
        );

        // 720p
        resolutions.insert(
            "720p".to_string(),
            ResolutionConfig {
                width: Some(1280),
                height: Some(720),
                video_bitrate: None,
                audio_bitrate: None,
                video_codec: Some("libx265".to_string()),
                audio_codec: Some("copy".to_string()),
                quality: Some(65),
                is_original: false,
            },
        );

        // 1080p (original)
        resolutions.insert(
            "1080p".to_string(),
            ResolutionConfig {
                width: None,
                height: None,
                video_bitrate: None,
                audio_bitrate: None,
                video_codec: Some("copy".to_string()),
                audio_codec: Some("copy".to_string()),
                quality: None,
                is_original: true,
            },
        );

        Self {
            resolutions,
            hls_time: 6,
            hls_list_size: 0,
            segment_type: "fmp4".to_string(),
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

pub struct ProcessedVideo {
    pub master_playlist: PathBuf,
    pub stream_playlists: Vec<PathBuf>,
    pub segments: Vec<PathBuf>,
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

/// Process video to HLS format
pub async fn process_video(
    url: &str,
    output_dir: &Path,
    config: &TransformConfig,
) -> Result<ProcessedVideo, Box<dyn std::error::Error>> {
    log::info!("Processing video: {}", url);

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

    // Build FFmpeg command
    let output_pattern = output_dir.join("stream_%v.m3u8");
    let master_name = output_dir.join("master.m3u8");

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(url);

    // Build filter complex for multiple resolutions
    let mut filter_parts = Vec::new();
    let mut video_maps = Vec::new();
    let mut audio_maps = Vec::new();
    let mut var_stream_map_parts = Vec::new();

    let resolution_keys: Vec<String> = config.resolutions.keys().cloned().collect();
    let non_original_count = config
        .resolutions
        .values()
        .filter(|r| !r.is_original)
        .count();

    // Build split filter if needed
    if non_original_count > 0 {
        let split_outputs: Vec<String> = (0..non_original_count)
            .map(|i| format!("v{}", i))
            .collect();
        filter_parts.push(format!("[0:v]split={}[{}]", non_original_count, split_outputs.join("][")));
    }

    let mut stream_idx = 0;
    let mut split_idx = 0;

    for key in &resolution_keys {
        let res_config = &config.resolutions[key];

        if !res_config.is_original {
            // Add scale filter
            let input_label = format!("v{}", split_idx);
            let output_label = format!("v{}out", split_idx);

            if let (Some(width), Some(height)) = (res_config.width, res_config.height) {
                filter_parts.push(format!("[{}]scale={}:{}[{}]", input_label, width, height, output_label));

                // Map video stream
                cmd.arg("-map").arg(format!("[{}]", output_label));

                if let Some(codec) = &res_config.video_codec {
                    cmd.arg(format!("-c:v:{}", stream_idx)).arg(codec);
                }

                if let Some(quality) = res_config.quality {
                    cmd.arg(format!("-q:v:{}", stream_idx))
                        .arg(quality.to_string());
                }

                split_idx += 1;
            }
        } else {
            // Original stream - just copy
            cmd.arg("-map").arg("0:v");

            if let Some(codec) = &res_config.video_codec {
                cmd.arg(format!("-c:v:{}", stream_idx)).arg(codec);
            }
        }

        // Map audio stream
        cmd.arg("-map").arg("0:a");

        if let Some(codec) = &res_config.audio_codec {
            cmd.arg(format!("-c:a:{}", stream_idx)).arg(codec);
        }

        if let Some(bitrate) = &res_config.audio_bitrate {
            cmd.arg(format!("-b:a:{}", stream_idx)).arg(bitrate);
        }

        // Add to variant stream map
        var_stream_map_parts.push(format!("v:{},a:{}", stream_idx, stream_idx));

        stream_idx += 1;
    }

    // Add filter complex if we have filters
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

    log::info!("FFmpeg processing completed");

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
        master_playlist: master_name,
        stream_playlists,
        segments,
        metadata,
    })
}

/// Calculate SHA-256 hash of a file
pub async fn calculate_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let contents = fs::read(file_path).await?;
    let hash = Sha256::digest(&contents);
    Ok(format!("{:x}", hash))
}

/// Rename files to content-addressable names and update playlists
pub async fn make_content_addressable(
    processed: &ProcessedVideo,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    log::info!("Making files content-addressable");

    let mut hash_map: HashMap<String, String> = HashMap::new();

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
        update_playlist(playlist_path, &hash_map).await?;

        let hash = calculate_sha256(playlist_path).await?;
        let new_name = format!("{}.m3u8", hash);
        let new_path = playlist_path.with_file_name(&new_name);

        let old_name = playlist_path.file_name().unwrap().to_string_lossy().to_string();
        stream_hash_map.insert(old_name, new_name.clone());

        fs::rename(playlist_path, &new_path).await?;
        log::debug!("Renamed playlist: {} -> {}", playlist_path.display(), new_path.display());
    }

    // Step 3: Update master playlist
    update_playlist(&processed.master_playlist, &stream_hash_map).await?;

    // Calculate master playlist hash (but don't rename it - keep it as master.m3u8)
    let master_hash = calculate_sha256(&processed.master_playlist).await?;
    hash_map.insert("master.m3u8".to_string(), master_hash);

    log::info!("Content-addressable naming completed");
    Ok(hash_map)
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
