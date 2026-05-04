// Phase 1 stub — expanded in Phase 2
pub mod launchd;
pub mod process;
pub mod sysv;
pub mod systemd;

use crate::paths::Paths;

/// Detected service manager for this platform.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceManager {
    SystemdUser,
    SystemdSystem,
    Launchd,
    SysV,
    None,
}

impl ServiceManager {
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

pub fn install_and_start(_paths: &Paths, _system: bool, _force: bool, _user: Option<&str>) -> anyhow::Result<()> {
    anyhow::bail!("Service management not yet implemented — coming in Phase 2")
}
pub fn uninstall(_paths: &Paths, _system: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
}
pub fn start(_paths: &Paths, _system: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
}
pub fn stop(_system: bool, _force: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
}
pub fn restart(_paths: &Paths, _system: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
}
pub fn status(_system: bool, _deep: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
}
pub fn logs(_paths: &Paths, _follow: bool, _lines: u32, _system: bool) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented")
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
