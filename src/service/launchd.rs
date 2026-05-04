//! macOS launchd user agent management.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::info;

/// Generate a launchd user agent plist for nostube-transcode.
pub fn generate_plist(binary_path: &str, env_file: &str, log_dir: &str) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let local_bin = format!("{}/.local/bin", home.display());
    let path_val = format!(
        "{local_bin}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.nostube.transcode</string>

  <key>ProgramArguments</key>
  <array>
    <string>{binary_path}</string>
    <string>run</string>
    <string>--replace</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{path_val}</string>
    <key>ENV_FILE</key>
    <string>{env_file}</string>
  </dict>

  <key>RunAtLoad</key>
  <true/>

  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>

  <key>ThrottleInterval</key>
  <integer>30</integer>

  <key>StandardOutPath</key>
  <string>{log_dir}/stdout.log</string>

  <key>StandardErrorPath</key>
  <string>{log_dir}/stderr.log</string>
</dict>
</plist>
"#
    )
}

/// Returns true if the installed plist differs from `expected` or is missing.
pub fn is_plist_stale(plist_path: &Path, expected: &str) -> bool {
    match std::fs::read_to_string(plist_path) {
        Ok(current) => current != expected,
        Err(_) => true,
    }
}

/// Install or repair the launchd user agent.
pub fn install(paths: &crate::paths::Paths) -> Result<()> {
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();
    let log_dir = paths.log_dir.to_string_lossy().to_string();

    std::fs::create_dir_all(&paths.log_dir)
        .context("Failed to create log directory")?;

    let plist_content = generate_plist(&binary, &env_file, &log_dir);

    if let Some(parent) = paths.launchd_plist.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create LaunchAgents directory")?;
    }

    let uid = get_uid();

    // Unload existing if present (ignore error — may not be loaded)
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/com.nostube.transcode")])
        .status();

    std::fs::write(&paths.launchd_plist, &plist_content)
        .context("Failed to write launchd plist")?;
    info!("Wrote plist: {:?}", paths.launchd_plist);

    let plist_str = paths.launchd_plist.to_string_lossy().to_string();
    let status = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), &plist_str])
        .status()
        .context("launchctl not available")?;
    if !status.success() {
        bail!("launchctl bootstrap failed");
    }
    Ok(())
}

/// Start (kickstart) the launchd service.
pub fn start() -> Result<()> {
    let uid = get_uid();
    let status = Command::new("launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/com.nostube.transcode")])
        .status()
        .context("launchctl not available")?;
    if !status.success() {
        bail!("launchctl kickstart failed — is the service installed?");
    }
    Ok(())
}

/// Stop the launchd service.
pub fn stop() -> Result<()> {
    let uid = get_uid();
    let _ = Command::new("launchctl")
        .args([
            "kill",
            "SIGTERM",
            &format!("gui/{uid}/com.nostube.transcode"),
        ])
        .status();
    Ok(())
}

/// Restart (refresh plist if stale, then kickstart).
pub fn restart(paths: &crate::paths::Paths) -> Result<()> {
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();
    let log_dir = paths.log_dir.to_string_lossy().to_string();
    let expected = generate_plist(&binary, &env_file, &log_dir);

    if is_plist_stale(&paths.launchd_plist, &expected) {
        info!("Plist is stale — refreshing");
        let uid = get_uid();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/com.nostube.transcode")])
            .status();
        std::fs::write(&paths.launchd_plist, &expected)?;
        let plist_str = paths.launchd_plist.to_string_lossy().to_string();
        let status = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &plist_str])
            .status()
            .context("launchctl bootstrap failed")?;
        if !status.success() {
            bail!("launchctl bootstrap failed after plist refresh");
        }
    }
    start()
}

/// Uninstall the launchd user agent.
pub fn uninstall(paths: &crate::paths::Paths) -> Result<()> {
    let uid = get_uid();
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/com.nostube.transcode")])
        .status();
    if paths.launchd_plist.exists() {
        std::fs::remove_file(&paths.launchd_plist)?;
        info!("Removed plist: {:?}", paths.launchd_plist);
    }
    Ok(())
}

/// Print launchctl list entry for nostube.
pub fn status() -> Result<()> {
    Command::new("launchctl")
        .args(["list", "com.nostube.transcode"])
        .status()
        .context("launchctl not available")?;
    Ok(())
}

/// Tail log files.
pub fn logs(paths: &crate::paths::Paths, follow: bool, lines: u32) -> Result<()> {
    std::fs::create_dir_all(&paths.log_dir).ok();
    // Touch stderr log so tail doesn't fail on first run
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stderr_log);
    let n = lines.to_string();
    let stderr = paths.stderr_log.to_string_lossy().to_string();
    let mut args = vec!["-n", &n];
    if follow {
        args.insert(0, "-f");
    }
    args.push(&stderr);
    Command::new("tail")
        .args(&args)
        .status()
        .context("tail not available")?;
    Ok(())
}

fn get_uid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::getuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plist_content() {
        let binary = "/Users/alice/.local/bin/nostube-transcode";
        let env_file = "/Users/alice/.local/share/nostube-transcode/env";
        let log_dir = "/Users/alice/.local/share/nostube-transcode/logs";
        let plist = generate_plist(binary, env_file, log_dir);
        assert!(plist.contains("com.nostube.transcode"));
        assert!(plist.contains(binary));
        assert!(plist.contains("<string>run</string>"));
        assert!(plist.contains("<string>--replace</string>"));
        assert!(plist.contains("RunAtLoad"));
        assert!(plist.contains("SuccessfulExit"));
        assert!(plist.contains("stdout.log"));
        assert!(plist.contains("stderr.log"));
        assert!(plist.contains("/opt/homebrew/bin"));
    }

    #[test]
    fn test_plist_stale_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.plist");
        assert!(is_plist_stale(&path, "content"));
    }

    #[test]
    fn test_plist_stale_same() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.plist");
        std::fs::write(&path, "content").unwrap();
        assert!(!is_plist_stale(&path, "content"));
    }
}
