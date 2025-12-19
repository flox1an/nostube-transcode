use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::error::VideoError;

/// Rewrites M3U8 playlists to use hash-based filenames for Blossom uploads
pub struct PlaylistRewriter {
    /// Map from original filename to SHA-256 hash
    segment_hashes: HashMap<String, String>,
}

impl PlaylistRewriter {
    pub fn new() -> Self {
        Self {
            segment_hashes: HashMap::new(),
        }
    }

    /// Register a segment file with its hash
    pub fn add_segment(&mut self, original_name: &str, hash: &str) {
        self.segment_hashes
            .insert(original_name.to_string(), hash.to_string());
    }

    /// Rewrite a playlist file, replacing segment references with hash-based names
    pub async fn rewrite_playlist(&self, path: &Path) -> Result<String, VideoError> {
        let content = fs::read_to_string(path).await?;
        self.rewrite_content(&content)
    }

    /// Rewrite playlist content
    pub fn rewrite_content(&self, content: &str) -> Result<String, VideoError> {
        let uri_regex =
            Regex::new(r#"URI="([^"]+)""#).map_err(|e| VideoError::PlaylistParse(e.to_string()))?;
        let segment_regex = Regex::new(r"^([^#\s].*\.(m4s|ts|mp4))$")
            .map_err(|e| VideoError::PlaylistParse(e.to_string()))?;

        let mut output = String::new();

        for line in content.lines() {
            let new_line = if line.starts_with('#') {
                // Check for URI in tags like EXT-X-MAP
                if let Some(caps) = uri_regex.captures(line) {
                    let original = &caps[1];
                    if let Some(hash) = self.segment_hashes.get(original) {
                        let ext = Path::new(original)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("m4s");
                        line.replace(original, &format!("{}.{}", hash, ext))
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            } else if let Some(caps) = segment_regex.captures(line) {
                // Standalone segment filename
                let original = &caps[1];
                if let Some(hash) = self.segment_hashes.get(original) {
                    let ext = Path::new(original)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("m4s");
                    format!("{}.{}", hash, ext)
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            };

            output.push_str(&new_line);
            output.push('\n');
        }

        Ok(output)
    }

    /// Rewrite master playlist to use hash-based stream playlist names
    pub fn rewrite_master_playlist(
        &self,
        content: &str,
        playlist_hashes: &HashMap<String, String>,
    ) -> Result<String, VideoError> {
        let mut output = String::new();

        for line in content.lines() {
            let new_line = if line.starts_with('#') {
                line.to_string()
            } else if line.ends_with(".m3u8") {
                // Stream playlist reference
                if let Some(hash) = playlist_hashes.get(line) {
                    format!("{}.m3u8", hash)
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            };

            output.push_str(&new_line);
            output.push('\n');
        }

        Ok(output)
    }
}

impl Default for PlaylistRewriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_playlist() {
        let mut rewriter = PlaylistRewriter::new();
        rewriter.add_segment("stream_0_000.m4s", "abc123");
        rewriter.add_segment("stream_0_001.m4s", "def456");
        rewriter.add_segment("init_0.m4s", "init789");

        let content = r#"#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:6
#EXT-X-MAP:URI="init_0.m4s"
#EXTINF:6.000,
stream_0_000.m4s
#EXTINF:6.000,
stream_0_001.m4s
#EXT-X-ENDLIST
"#;

        let result = rewriter.rewrite_content(content).unwrap();

        assert!(result.contains("init789.m4s"));
        assert!(result.contains("abc123.m4s"));
        assert!(result.contains("def456.m4s"));
        assert!(!result.contains("stream_0_000"));
    }

    #[test]
    fn test_rewrite_master_playlist() {
        let rewriter = PlaylistRewriter::new();

        let mut playlist_hashes = HashMap::new();
        playlist_hashes.insert("stream_0.m3u8".to_string(), "hash0".to_string());
        playlist_hashes.insert("stream_1.m3u8".to_string(), "hash1".to_string());

        let content = r#"#EXTM3U
#EXT-X-VERSION:7
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360
stream_0.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1280x720
stream_1.m3u8
"#;

        let result = rewriter
            .rewrite_master_playlist(content, &playlist_hashes)
            .unwrap();

        assert!(result.contains("hash0.m3u8"));
        assert!(result.contains("hash1.m3u8"));
        assert!(!result.contains("stream_0.m3u8"));
    }
}
