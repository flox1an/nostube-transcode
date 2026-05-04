//! systemd user and system service management.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

/// Generate a systemd user unit file for nostube-transcode.
pub fn generate_user_unit(binary_path: &str, env_file: &str, home: &str) -> String {
    let cargo_bin = format!("{}/.cargo/bin", home);
    let local_bin = format!("{}/.local/bin", home);
    format!(
        "[Unit]\n\
         Description=nostube-transcode DVM\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         StartLimitIntervalSec=0\n\
         \n\
         [Service]\n\
         Type=simple\n\
         EnvironmentFile={env_file}\n\
         ExecStart={binary_path} run --replace\n\
         WorkingDirectory={home}\n\
         Environment=\"PATH={local_bin}:{cargo_bin}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"\n\
         Restart=always\n\
         RestartSec=30\n\
         KillMode=mixed\n\
         KillSignal=SIGTERM\n\
         TimeoutStopSec=90\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

/// Generate a systemd system unit file for nostube-transcode.
pub fn generate_system_unit(
    binary_path: &str,
    env_file: &str,
    home: &str,
    user: &str,
) -> String {
    let cargo_bin = format!("{}/.cargo/bin", home);
    let local_bin = format!("{}/.local/bin", home);
    format!(
        "[Unit]\n\
         Description=nostube-transcode DVM\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         StartLimitIntervalSec=0\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={user}\n\
         Group={user}\n\
         EnvironmentFile={env_file}\n\
         ExecStart={binary_path} run --replace\n\
         WorkingDirectory={home}\n\
         Environment=\"HOME={home}\"\n\
         Environment=\"USER={user}\"\n\
         Environment=\"PATH={local_bin}:{cargo_bin}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"\n\
         Restart=always\n\
         RestartSec=30\n\
         KillMode=mixed\n\
         KillSignal=SIGTERM\n\
         TimeoutStopSec=90\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

/// Returns true if the installed unit file content differs from `expected`.
/// Also true if the file is missing.
pub fn is_unit_stale(unit_path: &Path, expected: &str) -> bool {
    match std::fs::read_to_string(unit_path) {
        Ok(current) => current != expected,
        Err(_) => true,
    }
}

/// Install or repair the user systemd service.
pub fn install_user(paths: &crate::paths::Paths) -> Result<()> {
    let home = dirs::home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy();
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();

    let unit_content = generate_user_unit(&binary, &env_file, &home_str);

    if let Some(parent) = paths.systemd_user_unit.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create systemd user unit directory")?;
    }

    std::fs::write(&paths.systemd_user_unit, &unit_content)
        .context("Failed to write systemd user unit")?;
    info!("Wrote unit file: {:?}", paths.systemd_user_unit);

    run_systemctl_user(&["daemon-reload"])?;
    run_systemctl_user(&["enable", "nostube-transcode"])?;
    info!("Service enabled");
    Ok(())
}

/// Start the user systemd service.
pub fn start_user() -> Result<()> {
    run_systemctl_user(&["start", "nostube-transcode"])?;
    info!("Service started");
    Ok(())
}

/// Stop the user systemd service.
pub fn stop_user() -> Result<()> {
    run_systemctl_user(&["stop", "nostube-transcode"])?;
    Ok(())
}

/// Restart the user systemd service (refreshes stale unit first).
pub fn restart_user(paths: &crate::paths::Paths) -> Result<()> {
    let home = dirs::home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy();
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();
    let expected = generate_user_unit(&binary, &env_file, &home_str);
    if is_unit_stale(&paths.systemd_user_unit, &expected) {
        info!("Unit file is stale — refreshing");
        std::fs::write(&paths.systemd_user_unit, &expected)?;
        run_systemctl_user(&["daemon-reload"])?;
    }
    run_systemctl_user(&["restart", "nostube-transcode"])?;
    Ok(())
}

/// Uninstall the user systemd service.
pub fn uninstall_user(paths: &crate::paths::Paths) -> Result<()> {
    let _ = run_systemctl_user(&["stop", "nostube-transcode"]);
    let _ = run_systemctl_user(&["disable", "nostube-transcode"]);
    if paths.systemd_user_unit.exists() {
        std::fs::remove_file(&paths.systemd_user_unit)?;
        info!("Removed unit file: {:?}", paths.systemd_user_unit);
    }
    run_systemctl_user(&["daemon-reload"])?;
    Ok(())
}

/// Print status using `systemctl --user status`.
pub fn status_user(deep: bool) -> Result<()> {
    let args: &[&str] = if deep {
        &["--user", "status", "nostube-transcode"]
    } else {
        &["--user", "status", "--no-pager", "-l", "nostube-transcode"]
    };
    let status = Command::new("systemctl").args(args).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => bail!("Service is not running"),
        Err(e) => bail!("systemctl not available: {}", e),
    }
}

/// Follow or print logs using journalctl.
pub fn logs_user(follow: bool, lines: u32) -> Result<()> {
    let n = lines.to_string();
    let mut args = vec!["--user", "-u", "nostube-transcode", "-n", &n];
    if follow {
        args.push("-f");
    }
    let status = Command::new("journalctl")
        .args(&args)
        .status()
        .context("journalctl not available")?;
    if !status.success() {
        warn!("journalctl exited with non-zero status");
    }
    Ok(())
}

fn run_systemctl_user(args: &[&str]) -> Result<()> {
    let mut full_args = vec!["--user"];
    full_args.extend_from_slice(args);
    let status = Command::new("systemctl")
        .args(&full_args)
        .status()
        .context("systemctl not available")?;
    if !status.success() {
        bail!("systemctl {} failed", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_file_content_user() {
        let binary = "/home/alice/.local/bin/nostube-transcode";
        let env_file = "/home/alice/.local/share/nostube-transcode/env";
        let home = "/home/alice";
        let unit = generate_user_unit(binary, env_file, home);
        assert!(unit.contains(
            "ExecStart=/home/alice/.local/bin/nostube-transcode run --replace"
        ));
        assert!(unit
            .contains("EnvironmentFile=/home/alice/.local/share/nostube-transcode/env"));
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("After=network-online.target"));
        assert!(unit.contains("StartLimitIntervalSec=0"));
    }

    #[test]
    fn test_unit_file_content_system() {
        let binary = "/home/alice/.local/bin/nostube-transcode";
        let env_file = "/home/alice/.local/share/nostube-transcode/env";
        let home = "/home/alice";
        let unit = generate_system_unit(binary, env_file, home, "alice");
        assert!(unit.contains("User=alice"));
        assert!(unit.contains("WantedBy=multi-user.target"));
        assert!(unit.contains("HOME=/home/alice"));
    }

    #[test]
    fn test_stale_detection_identical_content() {
        let content = "[Unit]\nDescription=test\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.service");
        std::fs::write(&path, content).unwrap();
        assert!(!is_unit_stale(&path, content));
    }

    #[test]
    fn test_stale_detection_changed_content() {
        let old = "[Unit]\nDescription=old\n";
        let new = "[Unit]\nDescription=new\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.service");
        std::fs::write(&path, old).unwrap();
        assert!(is_unit_stale(&path, new));
    }

    #[test]
    fn test_stale_detection_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.service");
        assert!(is_unit_stale(&path, "[Unit]"));
    }
}
