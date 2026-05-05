//! Service manager detection and dispatch.

pub mod launchd;
pub mod process;
pub mod sysv;
pub mod systemd;

use anyhow::{bail, Result};
use crate::paths::Paths;

/// Detected service manager for this platform.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceManager {
    /// Linux systemd (user session)
    SystemdUser,
    /// Linux systemd (system-wide, requires root)
    SystemdSystem,
    /// macOS launchd user agent
    Launchd,
    /// SysV init (Linux without systemd)
    SysV,
    /// No supported service manager detected
    None,
}

impl ServiceManager {
    /// Detect the best available service manager for this platform.
    pub fn detect() -> Self {
        #[cfg(target_os = "macos")]
        return ServiceManager::Launchd;

        #[cfg(target_os = "linux")]
        {
            if is_systemd_available() {
                return ServiceManager::SystemdUser;
            }
            if std::path::Path::new("/etc/init.d").exists() {
                return ServiceManager::SysV;
            }
            return ServiceManager::None;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        ServiceManager::None
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::SystemdUser => "systemd (user)",
            Self::SystemdSystem => "systemd (system)",
            Self::Launchd => "launchd",
            Self::SysV => "SysV init",
            Self::None => "none",
        }
    }
}

/// Install and immediately start the service using the detected manager.
pub fn install_and_start(
    paths: &Paths,
    system: bool,
    _force: bool,
    _user: Option<&str>,
) -> Result<()> {
    let mgr = if system {
        ServiceManager::SystemdSystem
    } else {
        ServiceManager::detect()
    };

    match mgr {
        ServiceManager::SystemdUser => {
            systemd::install_user(paths)?;
            systemd::start_user()?;
            println!("Service installed and started (systemd user).");
            println!("Logs: nostube-transcode logs -f");
        }
        ServiceManager::Launchd => {
            launchd::install(paths)?;
            println!("Service installed and started (launchd).");
            println!("Logs: nostube-transcode logs -f");
        }
        ServiceManager::SysV => {
            sysv::write_script(paths)?;
            println!("SysV script written. Follow the instructions above to activate.");
        }
        ServiceManager::SystemdSystem => {
            bail!(
                "System service install via CLI is not yet implemented.\n\
                 Manual steps:\n\
                 1. sudo cp {} /etc/systemd/system/nostube-transcode.service\n\
                 2. sudo systemctl daemon-reload\n\
                 3. sudo systemctl enable --now nostube-transcode",
                paths.systemd_user_unit.display()
            );
        }
        ServiceManager::None => {
            bail!(
                "No supported service manager detected.\n\
                 Run in the foreground with: nostube-transcode run"
            );
        }
    }
    Ok(())
}

/// Stop and remove the service.
pub fn uninstall(paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::uninstall_user(paths),
        ServiceManager::Launchd => launchd::uninstall(paths),
        mgr => bail!("Uninstall not supported for {}", mgr.name()),
    }
}

/// Start the service.
pub fn start(_paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::start_user(),
        ServiceManager::Launchd => launchd::start(),
        ServiceManager::SysV => {
            println!("For SysV: sudo service nostube-transcode start");
            Ok(())
        }
        mgr => bail!("Start not supported for {} — run manually: nostube-transcode run", mgr.name()),
    }
}

/// Stop the service.
pub fn stop(_system: bool, _force: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::stop_user(),
        ServiceManager::Launchd => launchd::stop(),
        ServiceManager::SysV => {
            println!("For SysV: sudo service nostube-transcode stop");
            Ok(())
        }
        mgr => bail!("Stop not supported for {}", mgr.name()),
    }
}

/// Restart the service (refreshes stale service definition first).
pub fn restart(paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::restart_user(paths),
        ServiceManager::Launchd => launchd::restart(paths),
        ServiceManager::SysV => {
            println!("For SysV: sudo service nostube-transcode restart");
            Ok(())
        }
        mgr => bail!("Restart not supported for {}", mgr.name()),
    }
}

/// Print service status.
pub fn status(_system: bool, deep: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::status_user(deep),
        ServiceManager::Launchd => launchd::status(),
        ServiceManager::SysV => {
            println!("For SysV: sudo service nostube-transcode status");
            Ok(())
        }
        mgr => bail!("Status not supported for {}", mgr.name()),
    }
}

/// Print or follow logs.
pub fn logs(paths: &Paths, follow: bool, lines: u32, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::logs_user(follow, lines),
        ServiceManager::Launchd => launchd::logs(paths, follow, lines),
        _ => {
            // Fallback: try to tail the log file
            if paths.stderr_log.exists() {
                let n = lines.to_string();
                let stderr = paths.stderr_log.to_string_lossy().to_string();
                let mut args = vec!["-n", &n];
                if follow {
                    args.insert(0, "-f");
                }
                args.push(&stderr);
                std::process::Command::new("tail").args(&args).status().ok();
                Ok(())
            } else {
                bail!(
                    "No log file found at {}.\n\
                     Run the DVM in the foreground and check terminal output.",
                    paths.stderr_log.display()
                )
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn is_systemd_available() -> bool {
    std::path::Path::new("/run/systemd/system").exists()
        || std::process::Command::new("pidof")
            .arg("systemd")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}
