use std::fmt;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

/// Managed temporary directory that cleans up on drop.
pub struct TempDir {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl fmt::Debug for TempDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TempDir")
            .field("path", &self.path)
            .field("cleanup_on_drop", &self.cleanup_on_drop)
            .finish()
    }
}

impl TempDir {
    /// Create a new temporary directory under the given base path.
    pub async fn new(base: &Path) -> std::io::Result<Self> {
        let id = Uuid::new_v4();
        let path = base.join(format!("dvm-{}", id));
        fs::create_dir_all(&path).await?;

        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Get the path to the temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Disable cleanup on drop (useful for debugging).
    pub fn keep(&mut self) {
        self.cleanup_on_drop = false;
    }

    /// Manually clean up the directory.
    pub async fn cleanup(&self) -> std::io::Result<()> {
        if self.path.exists() {
            fs::remove_dir_all(&self.path).await?;
        }
        Ok(())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup_on_drop && self.path.exists() {
            // Use blocking remove since we're in drop
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
