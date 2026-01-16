use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::blossom::auth::create_upload_auth_token;
use crate::config::Config;
use crate::dvm::events::{HlsResult, StreamPlaylist};
use crate::error::BlossomError;
use crate::util::hash_file;
use crate::video::playlist::PlaylistRewriter;
use crate::video::TransformResult;

/// A wrapper around an AsyncRead that tracks bytes read via an atomic counter
pub struct ProgressReader<R> {
    inner: R,
    bytes_read: Arc<AtomicU64>,
}

impl<R> ProgressReader<R> {
    pub fn new(inner: R, bytes_read: Arc<AtomicU64>) -> Self {
        Self { inner, bytes_read }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        let after = buf.filled().len();
        let bytes_read = (after - before) as u64;
        if bytes_read > 0 {
            self.bytes_read.fetch_add(bytes_read, Ordering::Relaxed);
        }
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobDescriptor {
    /// Publicly accessible URL to the blob
    pub url: String,
    /// SHA-256 hash of the blob
    pub sha256: String,
    /// Size of the blob in bytes
    pub size: u64,
    /// MIME type (falls back to application/octet-stream if unknown)
    #[serde(rename = "type")]
    pub mime_type: String,
    /// Unix timestamp of when the blob was uploaded
    pub uploaded: i64,
}

pub struct BlossomClient {
    config: Arc<Config>,
    http: Client,
}

impl BlossomClient {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    /// Get the number of configured Blossom servers
    pub fn server_count(&self) -> usize {
        self.config.blossom_servers.len()
    }

    /// Upload a file to all configured Blossom servers
    /// Returns list of successful uploads (at least one required)
    pub async fn upload_file_to_all(
        &self,
        path: &Path,
        mime_type: &str,
    ) -> Result<Vec<BlobDescriptor>, BlossomError> {
        self.upload_file_to_all_with_progress(path, mime_type, |_, _| {})
            .await
    }

    /// Upload a file to all configured Blossom servers with progress callback
    /// The callback is called after each server upload with (bytes_uploaded, upload_duration)
    /// Returns list of successful uploads (at least one required)
    pub async fn upload_file_to_all_with_progress<F>(
        &self,
        path: &Path,
        mime_type: &str,
        mut on_progress: F,
    ) -> Result<Vec<BlobDescriptor>, BlossomError>
    where
        F: FnMut(u64, Duration),
    {
        let metadata = tokio::fs::metadata(path).await?;
        let file_size = metadata.len();
        let sha256 = hash_file(path).await?;

        debug!(path = %path.display(), sha256 = %sha256, "Uploading file to all servers");

        let mut results = Vec::new();

        for server in &self.config.blossom_servers {
            let upload_start = Instant::now();
            match self
                .upload_to_server(server, path, &sha256, file_size, mime_type)
                .await
            {
                Ok(blob) => {
                    let upload_duration = upload_start.elapsed();
                    on_progress(file_size, upload_duration);
                    info!(
                        url = %blob.url,
                        sha256 = %blob.sha256,
                        server = %server,
                        duration_ms = upload_duration.as_millis(),
                        "File uploaded successfully"
                    );
                    results.push(blob);
                }
                Err(e) => {
                    warn!(server = %server, error = %e, "Upload failed");
                }
            }
        }

        if results.is_empty() {
            return Err(BlossomError::UploadFailed(
                "All server uploads failed".into(),
            ));
        }

        Ok(results)
    }

    /// Upload a file to Blossom (first successful server)
    pub async fn upload_file(
        &self,
        path: &Path,
        mime_type: &str,
    ) -> Result<BlobDescriptor, BlossomError> {
        let results = self.upload_file_to_all(path, mime_type).await?;
        Ok(results.into_iter().next().unwrap())
    }

    /// Upload a file to all configured Blossom servers with real-time progress tracking
    /// The bytes_uploaded counter is updated in real-time as bytes are sent
    /// Returns list of successful uploads (at least one required)
    pub async fn upload_file_to_all_with_realtime_progress(
        &self,
        path: &Path,
        mime_type: &str,
        bytes_uploaded: Arc<AtomicU64>,
    ) -> Result<Vec<BlobDescriptor>, BlossomError> {
        let metadata = tokio::fs::metadata(path).await?;
        let file_size = metadata.len();
        let sha256 = hash_file(path).await?;

        debug!(path = %path.display(), sha256 = %sha256, "Uploading file to all servers");

        let mut results = Vec::new();

        for server in &self.config.blossom_servers {
            let upload_start = Instant::now();
            // Reset the counter for each server (since we're uploading the full file again)
            let server_bytes = Arc::new(AtomicU64::new(0));
            match self
                .upload_to_server_with_progress(server, path, &sha256, file_size, mime_type, server_bytes.clone())
                .await
            {
                Ok(blob) => {
                    let upload_duration = upload_start.elapsed();
                    // Add the bytes from this server to the total
                    bytes_uploaded.fetch_add(file_size, Ordering::Relaxed);
                    info!(
                        url = %blob.url,
                        sha256 = %blob.sha256,
                        server = %server,
                        duration_ms = upload_duration.as_millis(),
                        "File uploaded successfully"
                    );
                    results.push(blob);
                }
                Err(e) => {
                    warn!(server = %server, error = %e, "Upload failed");
                }
            }
        }

        if results.is_empty() {
            return Err(BlossomError::UploadFailed(
                "All server uploads failed".into(),
            ));
        }

        Ok(results)
    }

    /// Upload a file to a single server with progress tracking
    /// The bytes_uploaded counter is updated in real-time as bytes are sent
    pub async fn upload_to_server_streaming_progress(
        &self,
        path: &Path,
        mime_type: &str,
        bytes_uploaded: Arc<AtomicU64>,
    ) -> Result<Vec<BlobDescriptor>, BlossomError> {
        let metadata = tokio::fs::metadata(path).await?;
        let file_size = metadata.len();
        let sha256 = hash_file(path).await?;

        debug!(path = %path.display(), sha256 = %sha256, "Uploading file with progress tracking");

        let mut results = Vec::new();

        for server in &self.config.blossom_servers {
            let upload_start = Instant::now();
            match self
                .upload_to_server_with_progress(server, path, &sha256, file_size, mime_type, bytes_uploaded.clone())
                .await
            {
                Ok(blob) => {
                    let upload_duration = upload_start.elapsed();
                    info!(
                        url = %blob.url,
                        sha256 = %blob.sha256,
                        server = %server,
                        duration_ms = upload_duration.as_millis(),
                        "File uploaded successfully"
                    );
                    results.push(blob);
                }
                Err(e) => {
                    warn!(server = %server, error = %e, "Upload failed");
                }
            }
        }

        if results.is_empty() {
            return Err(BlossomError::UploadFailed(
                "All server uploads failed".into(),
            ));
        }

        Ok(results)
    }

    async fn upload_to_server(
        &self,
        server: &Url,
        path: &Path,
        sha256: &str,
        size: u64,
        mime_type: &str,
    ) -> Result<BlobDescriptor, BlossomError> {
        let dummy_counter = Arc::new(AtomicU64::new(0));
        self.upload_to_server_with_progress(server, path, sha256, size, mime_type, dummy_counter).await
    }

    async fn upload_to_server_with_progress(
        &self,
        server: &Url,
        path: &Path,
        sha256: &str,
        size: u64,
        mime_type: &str,
        bytes_uploaded: Arc<AtomicU64>,
    ) -> Result<BlobDescriptor, BlossomError> {
        let auth_token = create_upload_auth_token(&self.config.nostr_keys, size, sha256)?;

        let file = File::open(path).await?;
        let progress_reader = ProgressReader::new(file, bytes_uploaded);
        let stream = ReaderStream::new(progress_reader);
        let body = reqwest::Body::wrap_stream(stream);

        let url = server.join("/upload")?;

        debug!(
            url = %url,
            path = %path.display(),
            size = size,
            sha256 = %sha256,
            mime_type = %mime_type,
            "Sending upload request to Blossom"
        );

        let response = self
            .http
            .put(url.clone())
            .header("Content-Type", mime_type)
            .header("Authorization", format!("Nostr {}", auth_token))
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let headers = response.headers().clone();

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(
                url = %url,
                status = %status,
                response_body = %text,
                response_headers = ?headers,
                path = %path.display(),
                size = size,
                sha256 = %sha256,
                "Blossom upload failed"
            );
            return Err(BlossomError::UploadFailed(format!(
                "{}: {}",
                status, text
            )));
        }

        let response_text = response.text().await?;
        debug!(
            url = %url,
            status = %status,
            response_body = %response_text,
            "Blossom upload response"
        );

        let blob: BlobDescriptor = serde_json::from_str(&response_text).map_err(|e| {
            error!(
                url = %url,
                response_body = %response_text,
                error = %e,
                "Failed to parse Blossom response JSON"
            );
            BlossomError::UploadFailed(format!("Invalid JSON response: {}", e))
        })?;

        Ok(blob)
    }

    /// Upload all HLS output files to Blossom
    pub async fn upload_hls_output(
        &self,
        result: &TransformResult,
    ) -> Result<HlsResult, BlossomError> {
        self.upload_hls_output_with_progress(result, |_, _| {}).await
    }

    /// Upload all HLS output files to Blossom with progress callback
    /// The callback is called after each file upload with (bytes_uploaded, upload_duration)
    pub async fn upload_hls_output_with_progress<F>(
        &self,
        result: &TransformResult,
        mut on_progress: F,
    ) -> Result<HlsResult, BlossomError>
    where
        F: FnMut(u64, Duration),
    {
        let mut rewriter = PlaylistRewriter::new();
        let mut playlist_hashes: HashMap<String, String> = HashMap::new();
        let mut stream_playlist_urls: HashMap<String, String> = HashMap::new();
        let mut stream_sizes: HashMap<String, u64> = HashMap::new();
        let mut total_size: u64 = 0;

        // Regex to extract stream index from segment filenames (e.g., "stream_0_001.m4s" -> "0")
        let stream_idx_regex = Regex::new(r"^(?:stream_|init_)(\d+)").ok();

        // Upload all segment files first
        for segment_path in &result.segment_paths {
            let sha256 = hash_file(segment_path).await?;
            let filename = segment_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            // Track size per stream
            let file_size = tokio::fs::metadata(segment_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            total_size += file_size;

            // Extract stream index and accumulate size
            if let Some(caps) = stream_idx_regex.as_ref().and_then(|re| re.captures(filename)) {
                let stream_idx = &caps[1];
                let playlist_name = format!("stream_{}.m3u8", stream_idx);
                *stream_sizes.entry(playlist_name).or_insert(0) += file_size;
            }

            rewriter.add_segment(filename, &sha256);

            // Upload the segment and track timing
            let upload_start = Instant::now();
            self.upload_file(segment_path, "video/mp4").await?;
            let upload_duration = upload_start.elapsed();
            on_progress(file_size, upload_duration);
        }

        // Rewrite and upload stream playlists
        for playlist_path in &result.stream_playlists {
            let rewritten = rewriter.rewrite_playlist(playlist_path).await?;

            // Write rewritten playlist to temp file
            let temp_path = playlist_path.with_extension("rewritten.m3u8");
            tokio::fs::write(&temp_path, &rewritten).await?;

            // Track playlist size
            let playlist_size = rewritten.len() as u64;
            total_size += playlist_size;

            let original_name = playlist_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            // Add playlist size to stream total
            *stream_sizes.entry(original_name.to_string()).or_insert(0) += playlist_size;

            // Upload and track hash with timing
            let upload_start = Instant::now();
            let blob = self
                .upload_file(&temp_path, "application/vnd.apple.mpegurl")
                .await?;
            let upload_duration = upload_start.elapsed();
            on_progress(playlist_size, upload_duration);

            playlist_hashes.insert(original_name.to_string(), blob.sha256);
            stream_playlist_urls.insert(original_name.to_string(), blob.url);

            // Clean up temp file
            let _ = tokio::fs::remove_file(&temp_path).await;
        }

        // Read master playlist to extract resolution info
        let master_content = tokio::fs::read_to_string(&result.master_playlist_path).await?;
        let stream_playlists =
            self.parse_stream_resolutions(&master_content, &stream_playlist_urls, &stream_sizes);

        // Rewrite and upload master playlist
        let rewritten_master =
            rewriter.rewrite_master_playlist(&master_content, &playlist_hashes)?;

        let temp_master = result.master_playlist_path.with_extension("rewritten.m3u8");
        tokio::fs::write(&temp_master, &rewritten_master).await?;

        // Add master playlist size to total
        let master_size = rewritten_master.len() as u64;
        total_size += master_size;

        let upload_start = Instant::now();
        let master_blob = self
            .upload_file(&temp_master, "application/vnd.apple.mpegurl")
            .await?;
        let upload_duration = upload_start.elapsed();
        on_progress(master_size, upload_duration);

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_master).await;

        Ok(HlsResult {
            master_playlist: master_blob.url,
            stream_playlists,
            total_size_bytes: total_size,
            encryption_key: Some(result.encryption_key.clone()),
        })
    }

    /// Parse master playlist to extract resolution and codecs for each stream playlist
    fn parse_stream_resolutions(
        &self,
        master_content: &str,
        playlist_urls: &HashMap<String, String>,
        stream_sizes: &HashMap<String, u64>,
    ) -> Vec<StreamPlaylist> {
        let resolution_regex = Regex::new(r"RESOLUTION=(\d+x\d+)").ok();
        let codecs_regex = Regex::new(r#"CODECS="([^"]+)""#).ok();
        let mut results = Vec::new();
        let mut current_resolution: Option<String> = None;
        let mut current_codecs: Option<String> = None;

        for line in master_content.lines() {
            if line.starts_with("#EXT-X-STREAM-INF:") {
                // Extract resolution from this line
                current_resolution = resolution_regex
                    .as_ref()
                    .and_then(|re| re.captures(line))
                    .map(|caps| caps[1].to_string());

                // Extract codecs from this line
                current_codecs = codecs_regex
                    .as_ref()
                    .and_then(|re| re.captures(line))
                    .map(|caps| caps[1].to_string());
            } else if line.ends_with(".m3u8") && !line.starts_with('#') {
                // This is a playlist reference
                if let Some(url) = playlist_urls.get(line) {
                    let resolution = current_resolution
                        .take()
                        .map(|r| {
                            // Convert "1280x720" to "720p"
                            r.split('x')
                                .nth(1)
                                .map(|h| format!("{}p", h))
                                .unwrap_or(r)
                        })
                        .unwrap_or_else(|| "unknown".to_string());

                    let size_bytes = stream_sizes.get(line).copied().unwrap_or(0);

                    // Build mimetype with codecs if available
                    let mimetype = current_codecs.take().map(|codecs| {
                        format!("video/mp4; codecs=\"{}\"", codecs)
                    });

                    results.push(StreamPlaylist {
                        url: url.clone(),
                        resolution,
                        size_bytes,
                        mimetype,
                    });
                }
                current_resolution = None;
                current_codecs = None;
            }
        }

        // Sort by resolution (descending)
        results.sort_by(|a, b| {
            let a_height: u32 = a.resolution.trim_end_matches('p').parse().unwrap_or(0);
            let b_height: u32 = b.resolution.trim_end_matches('p').parse().unwrap_or(0);
            b_height.cmp(&a_height)
        });

        results
    }

    /// List blobs uploaded by this DVM
    pub async fn list_blobs(&self, server: &Url) -> Result<Vec<BlobDescriptor>, BlossomError> {
        let auth_token = crate::blossom::auth::create_list_auth_token(&self.config.nostr_keys)?;

        let pubkey = self.config.nostr_keys.public_key();
        let url = server.join(&format!("/list/{}", pubkey.to_hex()))?;

        let response = self
            .http
            .get(url)
            .header("Authorization", format!("Nostr {}", auth_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(BlossomError::UploadFailed(format!("List failed: {}", text)));
        }

        let blobs: Vec<BlobDescriptor> = response.json().await?;
        Ok(blobs)
    }

    /// Delete a blob by its hash
    pub async fn delete_blob(&self, server: &Url, sha256: &str) -> Result<(), BlossomError> {
        let auth_token = crate::blossom::auth::create_delete_auth_token(&self.config.nostr_keys, sha256)?;

        let url = server.join(&format!("/{}", sha256))?;

        let response = self
            .http
            .delete(url)
            .header("Authorization", format!("Nostr {}", auth_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(BlossomError::UploadFailed(format!(
                "Delete failed: {}",
                text
            )));
        }

        Ok(())
    }
}
