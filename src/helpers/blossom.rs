use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use nostr::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

use crate::r#const::BLOSSOM_AUTH_KIND;

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobDescriptor {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub created: u64,
}

#[derive(Debug)]
pub struct BlossomClient {
    client: Client,
    keys: Keys,
}

impl BlossomClient {
    pub fn new(keys: Keys) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("Failed to create HTTP client"),
            keys,
        }
    }

    /// Generate Blossom authentication token
    fn generate_auth_token(
        &self,
        action: &str,
        tags: Vec<Tag>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let event = EventBuilder::new(Kind::from(BLOSSOM_AUTH_KIND), action, tags)
            .to_event(&self.keys)?;

        let json = event.as_json();
        let encoded = BASE64.encode(json.as_bytes());
        Ok(encoded)
    }

    /// Upload a file to a Blossom server
    pub async fn upload_file(
        &self,
        server_url: &str,
        file_path: &Path,
        sha256_hash: &str,
    ) -> Result<BlobDescriptor, Box<dyn std::error::Error>> {
        let file_bytes = fs::read(file_path).await?;
        let file_size = file_bytes.len() as u64;

        // Determine MIME type
        let mime_type = get_mime_type(file_path);

        // Generate auth token
        let expiration = Timestamp::now().as_u64() + 600; // 10 minutes
        let tags = vec![
            Tag::custom(TagKind::Custom("t".into()), vec!["upload"]),
            Tag::custom(TagKind::Custom("size".into()), vec![file_size.to_string()]),
            Tag::custom(TagKind::Custom("x".into()), vec![sha256_hash.to_string()]),
            Tag::custom(
                TagKind::Custom("name".into()),
                vec![file_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()],
            ),
            Tag::custom(
                TagKind::Custom("expiration".into()),
                vec![expiration.to_string()],
            ),
        ];

        let auth_token = self.generate_auth_token("Upload", tags)?;

        // Upload file
        let url = format!("{}/upload", server_url.trim_end_matches('/'));
        let response = self
            .client
            .put(&url)
            .header("Content-Type", &mime_type)
            .header("Authorization", format!("Nostr {}", auth_token))
            .body(file_bytes)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Upload failed with status {}: {}", status, text).into());
        }

        let blob: BlobDescriptor = response.json().await?;
        log::info!("Uploaded {} to {}", file_path.display(), blob.url);

        Ok(blob)
    }

    /// List all blobs owned by this DVM
    pub async fn list_blobs(
        &self,
        server_url: &str,
    ) -> Result<Vec<BlobDescriptor>, Box<dyn std::error::Error>> {
        let pubkey = self.keys.public_key();

        // Generate auth token
        let expiration = Timestamp::now().as_u64() + 600;
        let tags = vec![
            Tag::custom(TagKind::Custom("t".into()), vec!["list"]),
            Tag::custom(
                TagKind::Custom("expiration".into()),
                vec![expiration.to_string()],
            ),
        ];

        let auth_token = self.generate_auth_token("List Blobs", tags)?;

        // List blobs
        let url = format!(
            "{}/list/{}",
            server_url.trim_end_matches('/'),
            pubkey.to_hex()
        );
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Nostr {}", auth_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("List failed with status {}: {}", status, text).into());
        }

        let blobs: Vec<BlobDescriptor> = response.json().await?;
        Ok(blobs)
    }

    /// Delete a blob by its SHA-256 hash
    pub async fn delete_blob(
        &self,
        server_url: &str,
        sha256_hash: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Generate auth token
        let expiration = Timestamp::now().as_u64() + 600;
        let tags = vec![
            Tag::custom(TagKind::Custom("t".into()), vec!["delete"]),
            Tag::custom(TagKind::Custom("x".into()), vec![sha256_hash.to_string()]),
            Tag::custom(
                TagKind::Custom("expiration".into()),
                vec![expiration.to_string()],
            ),
        ];

        let auth_token = self.generate_auth_token("Delete Blob", tags)?;

        // Delete blob
        let url = format!(
            "{}/{}",
            server_url.trim_end_matches('/'),
            sha256_hash
        );
        let response = self
            .client
            .delete(&url)
            .header("Authorization", format!("Nostr {}", auth_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Delete failed with status {}: {}", status, text).into());
        }

        log::info!("Deleted blob: {}", sha256_hash);
        Ok(())
    }

    /// Clean up old blobs
    pub async fn cleanup_old_blobs(
        &self,
        server_url: &str,
        expiration_days: u64,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let blobs = self.list_blobs(server_url).await?;
        let cutoff = Timestamp::now().as_u64() - (60 * 60 * 24 * expiration_days);

        let mut deleted_count = 0;

        for blob in blobs {
            if blob.created < cutoff {
                match self.delete_blob(server_url, &blob.sha256).await {
                    Ok(_) => deleted_count += 1,
                    Err(e) => log::error!("Failed to delete blob {}: {}", blob.sha256, e),
                }
            }
        }

        log::info!(
            "Cleanup: deleted {} blobs older than {} days",
            deleted_count,
            expiration_days
        );

        Ok(deleted_count)
    }
}

/// Determine MIME type based on file extension
fn get_mime_type(file_path: &Path) -> String {
    match file_path.extension().and_then(|e| e.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl".to_string(),
        Some("m4s") => "video/iso.segment".to_string(),
        Some("ts") => "video/m2ts".to_string(),
        Some("mp4") => "video/mp4".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
