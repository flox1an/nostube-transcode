use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

use crate::error::ConfigError;

/// Discovered FFmpeg binary paths
#[derive(Debug, Clone)]
pub struct FfmpegPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl FfmpegPaths {
    /// Discover FFmpeg and FFprobe binaries.
    /// Search order:
    /// 1. Environment variables (FFMPEG_PATH, FFPROBE_PATH)
    /// 2. Platform-specific common locations
    /// 3. System PATH
    pub fn discover() -> Result<Self, ConfigError> {
        let ffmpeg = Self::find_ffmpeg()?;
        let ffprobe = Self::find_ffprobe()?;

        info!(ffmpeg = %ffmpeg.display(), ffprobe = %ffprobe.display(), "FFmpeg binaries discovered");

        Ok(Self { ffmpeg, ffprobe })
    }

    fn find_ffmpeg() -> Result<PathBuf, ConfigError> {
        // 1. Check environment variable
        if let Ok(path) = std::env::var("FFMPEG_PATH") {
            let path = PathBuf::from(path);
            if Self::validate_binary(&path, "ffmpeg") {
                debug!(path = %path.display(), "FFmpeg found via FFMPEG_PATH");
                return Ok(path);
            }
        }

        // 2. Check platform-specific locations
        for path in Self::ffmpeg_search_paths() {
            if Self::validate_binary(&path, "ffmpeg") {
                debug!(path = %path.display(), "FFmpeg found in common location");
                return Ok(path);
            }
        }

        // 3. Check system PATH
        if let Some(path) = Self::find_in_path("ffmpeg") {
            debug!(path = %path.display(), "FFmpeg found in PATH");
            return Ok(path);
        }

        Err(ConfigError::FfmpegNotFound(
            Self::ffmpeg_search_paths()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }

    fn find_ffprobe() -> Result<PathBuf, ConfigError> {
        // 1. Check environment variable
        if let Ok(path) = std::env::var("FFPROBE_PATH") {
            let path = PathBuf::from(path);
            if Self::validate_binary(&path, "ffprobe") {
                debug!(path = %path.display(), "FFprobe found via FFPROBE_PATH");
                return Ok(path);
            }
        }

        // 2. Check platform-specific locations
        for path in Self::ffprobe_search_paths() {
            if Self::validate_binary(&path, "ffprobe") {
                debug!(path = %path.display(), "FFprobe found in common location");
                return Ok(path);
            }
        }

        // 3. Check system PATH
        if let Some(path) = Self::find_in_path("ffprobe") {
            debug!(path = %path.display(), "FFprobe found in PATH");
            return Ok(path);
        }

        Err(ConfigError::FfprobeNotFound(
            Self::ffprobe_search_paths()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }

    /// Validate that a binary exists and is executable
    fn validate_binary(path: &PathBuf, expected_name: &str) -> bool {
        if !path.exists() {
            return false;
        }

        // Run -version to verify it's actually the right binary
        let output = Command::new(path).arg("-version").output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.to_lowercase().contains(expected_name)
            }
            _ => false,
        }
    }

    /// Find a binary in the system PATH
    fn find_in_path(name: &str) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        let name = format!("{}.exe", name);

        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(&name))
                .find(|path| path.exists())
        })
    }

    /// Platform-specific search paths for FFmpeg
    fn ffmpeg_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            // Windows common locations
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                paths.push(PathBuf::from(format!(
                    "{}\\ffmpeg\\bin\\ffmpeg.exe",
                    local_app_data
                )));
            }
            paths.push(PathBuf::from("C:\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from("C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from(".\\ffmpeg\\bin\\ffmpeg.exe"));
            paths.push(PathBuf::from(".\\ffmpeg.exe"));
        }

        #[cfg(target_os = "macos")]
        {
            // macOS Homebrew locations
            paths.push(PathBuf::from("/opt/homebrew/bin/ffmpeg")); // ARM
            paths.push(PathBuf::from("/usr/local/bin/ffmpeg")); // Intel
        }

        #[cfg(target_os = "linux")]
        {
            // Linux common locations
            paths.push(PathBuf::from("/usr/bin/ffmpeg"));
            paths.push(PathBuf::from("/usr/local/bin/ffmpeg"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(format!("{}/.local/bin/ffmpeg", home)));
            }
        }

        paths
    }

    /// Platform-specific search paths for FFprobe
    fn ffprobe_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                paths.push(PathBuf::from(format!(
                    "{}\\ffmpeg\\bin\\ffprobe.exe",
                    local_app_data
                )));
            }
            paths.push(PathBuf::from("C:\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from("C:\\Program Files\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from(".\\ffmpeg\\bin\\ffprobe.exe"));
            paths.push(PathBuf::from(".\\ffprobe.exe"));
        }

        #[cfg(target_os = "macos")]
        {
            paths.push(PathBuf::from("/opt/homebrew/bin/ffprobe"));
            paths.push(PathBuf::from("/usr/local/bin/ffprobe"));
        }

        #[cfg(target_os = "linux")]
        {
            paths.push(PathBuf::from("/usr/bin/ffprobe"));
            paths.push(PathBuf::from("/usr/local/bin/ffprobe"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(format!("{}/.local/bin/ffprobe", home)));
            }
        }

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_finds_ffmpeg() {
        // This test will pass if FFmpeg is installed on the system
        let result = FfmpegPaths::discover();
        // Don't assert success - FFmpeg might not be installed in CI
        if let Ok(paths) = result {
            assert!(paths.ffmpeg.exists());
            assert!(paths.ffprobe.exists());
        }
    }

    #[test]
    fn test_search_paths_not_empty() {
        let ffmpeg_paths = FfmpegPaths::ffmpeg_search_paths();
        let ffprobe_paths = FfmpegPaths::ffprobe_search_paths();

        assert!(!ffmpeg_paths.is_empty());
        assert!(!ffprobe_paths.is_empty());
    }
}
