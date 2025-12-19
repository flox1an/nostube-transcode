use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Compute SHA-256 hash of a file using streaming to handle large files.
pub async fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_hash_file() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();

        // Write test content
        let mut file = tokio::fs::File::create(&path).await.unwrap();
        file.write_all(b"hello world").await.unwrap();
        file.flush().await.unwrap();
        drop(file);

        let hash = hash_file(&path).await.unwrap();

        // SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
