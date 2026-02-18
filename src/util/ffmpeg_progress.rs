use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;

pub struct FfmpegProgressTracker {
    pub progress_ms: Arc<AtomicU64>,
}

impl FfmpegProgressTracker {
    pub fn new() -> Self {
        Self {
            progress_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn track_progress(&self, stdout: ChildStdout) -> tokio::io::Result<()> {
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            if line.starts_with("out_time_ms=") {
                if let Ok(ms) = line["out_time_ms=".len()..].parse::<i64>() {
                    // FFmpeg can sometimes output negative values at the start
                    let ms = ms.max(0) as u64;
                    self.progress_ms.store(ms, Ordering::Relaxed);
                }
            } else if line.starts_with("progress=") && line.ends_with("end") {
                // Done
                break;
            }
        }

        Ok(())
    }
}
