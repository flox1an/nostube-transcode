//! `nostube-transcode update` — self-update from GitHub releases.

use anyhow::{bail, Context, Result};
use std::path::Path;

const REPO: &str = "flox1an/nostube-transcode";
const GITHUB_API: &str = "https://api.github.com";

#[derive(Debug, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Detect the target triple for this binary to pick the right asset.
fn target_triple() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "x86_64-unknown-linux-musl";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "aarch64-unknown-linux-musl";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "x86_64-apple-darwin";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "aarch64-apple-darwin";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return "unknown";
}

/// Fetch the latest release info from GitHub.
async fn fetch_latest_release() -> Result<GithubRelease> {
    let url = format!("{GITHUB_API}/repos/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent(format!("nostube-transcode/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to reach GitHub API")?;
    if !resp.status().is_success() {
        bail!("GitHub API returned {}", resp.status());
    }
    resp.json::<GithubRelease>()
        .await
        .context("Failed to parse GitHub release JSON")
}

/// Run the update — check for newer version, download and replace binary.
pub async fn run(yes: bool, check_only: bool) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: v{current_version}");
    println!("Checking GitHub for latest release…");

    let release = fetch_latest_release()
        .await
        .context("Could not fetch latest release")?;

    let latest_tag = release.tag_name.trim_start_matches('v');
    println!("Latest release:  {}", release.tag_name);

    if latest_tag == current_version {
        println!("Already up to date.");
        return Ok(());
    }

    if check_only {
        println!("Update available: v{current_version} → {}", release.tag_name);
        return Ok(());
    }

    // Find the asset matching our target triple
    let triple = target_triple();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.contains(triple) && !a.name.ends_with(".sha256"))
        .with_context(|| {
            format!(
                "No release asset found for target '{}'. Available: {}",
                triple,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    println!("Asset: {}", asset.name);

    if !yes {
        use std::io::{self, Write};
        print!("Update to {}? [Y/n] ", release.tag_name);
        io::stdout().flush().ok();
        let mut line = String::new();
        io::stdin().read_line(&mut line).ok();
        let answer = line.trim().to_lowercase();
        if !answer.is_empty() && answer != "y" && answer != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("Downloading {}…", asset.browser_download_url);

    let client = reqwest::Client::builder()
        .user_agent(format!("nostube-transcode/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("Download failed")?
        .bytes()
        .await
        .context("Failed to read download body")?;

    let current_exe = std::env::current_exe().context("Could not determine current binary path")?;
    replace_binary(&current_exe, &bytes)?;

    println!(
        "Updated to {} — restart the service to apply:\n  nostube-transcode restart",
        release.tag_name
    );
    Ok(())
}

/// Atomically replace the running binary with new bytes.
fn replace_binary(target: &Path, data: &[u8]) -> Result<()> {
    // Write to a temp file next to the target
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, data).context("Failed to write new binary")?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }

    // Atomic rename
    std::fs::rename(&tmp, target).context("Failed to replace binary")?;
    Ok(())
}
