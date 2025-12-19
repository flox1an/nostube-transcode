use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

use crate::blossom::BlossomClient;
use crate::config::Config;

pub struct BlobCleanup {
    config: Arc<Config>,
    client: Arc<BlossomClient>,
}

impl BlobCleanup {
    pub fn new(config: Arc<Config>, client: Arc<BlossomClient>) -> Self {
        Self { config, client }
    }

    /// Run the cleanup scheduler
    pub async fn run(&self) {
        info!("Blob cleanup scheduler started");

        // Run cleanup daily
        let mut interval = interval(TokioDuration::from_secs(24 * 60 * 60));

        loop {
            interval.tick().await;

            if let Err(e) = self.cleanup_expired_blobs().await {
                error!(error = %e, "Blob cleanup failed");
            }
        }
    }

    /// Clean up expired blobs from all Blossom servers
    pub async fn cleanup_expired_blobs(&self) -> Result<usize, crate::error::BlossomError> {
        let expiration_threshold = Utc::now()
            - Duration::days(self.config.blob_expiration_days as i64);
        let threshold_ts = expiration_threshold.timestamp();

        info!(
            threshold = %expiration_threshold,
            days = self.config.blob_expiration_days,
            "Starting blob cleanup"
        );

        let mut total_deleted = 0;

        for server in &self.config.blossom_servers {
            match self.cleanup_server(server, threshold_ts).await {
                Ok(count) => {
                    total_deleted += count;
                    debug!(server = %server, deleted = count, "Server cleanup complete");
                }
                Err(e) => {
                    warn!(server = %server, error = %e, "Failed to cleanup server");
                }
            }
        }

        info!(total_deleted = total_deleted, "Blob cleanup complete");
        Ok(total_deleted)
    }

    async fn cleanup_server(
        &self,
        server: &url::Url,
        threshold_ts: i64,
    ) -> Result<usize, crate::error::BlossomError> {
        let blobs = self.client.list_blobs(server).await?;

        let expired: Vec<_> = blobs
            .iter()
            .filter(|b| b.uploaded < threshold_ts)
            .collect();

        debug!(
            server = %server,
            total = blobs.len(),
            expired = expired.len(),
            "Found blobs"
        );

        let mut deleted = 0;

        for blob in expired {
            match self.client.delete_blob(server, &blob.sha256).await {
                Ok(_) => {
                    deleted += 1;
                    debug!(sha256 = %blob.sha256, "Deleted expired blob");
                }
                Err(e) => {
                    warn!(sha256 = %blob.sha256, error = %e, "Failed to delete blob");
                }
            }
        }

        Ok(deleted)
    }
}
