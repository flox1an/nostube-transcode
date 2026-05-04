//! Central path resolution for nostube-transcode.
//!
//! All paths in one place so CLI commands, service managers, and setup
//! always agree on where to find config, identity, logs, and service files.

use std::path::PathBuf;

/// All filesystem paths used by nostube-transcode.
pub struct Paths {
    /// Data directory: ~/.local/share/nostube-transcode (or $DATA_DIR)
    pub data_dir: PathBuf,
    /// Env file: $data_dir/env
    pub env_file: PathBuf,
    /// Identity keypair: $data_dir/identity.key
    pub identity_file: PathBuf,
    /// PID file for foreground/fallback process tracking
    pub pid_file: PathBuf,
    /// Log directory: $data_dir/logs
    pub log_dir: PathBuf,
    /// stdout log (launchd/manual): $data_dir/logs/stdout.log
    pub stdout_log: PathBuf,
    /// stderr log (launchd/manual): $data_dir/logs/stderr.log
    pub stderr_log: PathBuf,
    /// Install directory: ~/.local/bin
    pub install_dir: PathBuf,
    /// Full path to installed binary: $install_dir/nostube-transcode
    pub binary_path: PathBuf,
    /// systemd user unit: ~/.config/systemd/user/nostube-transcode.service
    pub systemd_user_unit: PathBuf,
    /// launchd user agent plist: ~/Library/LaunchAgents/com.nostube.transcode.plist
    pub launchd_plist: PathBuf,
    /// SysV init script: $data_dir/nostube-transcode.initd
    pub sysv_script: PathBuf,
}

impl Paths {
    /// Resolve all paths using environment variables and XDG conventions.
    pub fn resolve() -> Self {
        let data_dir = crate::identity::default_data_dir();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        let config_base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"));

        let install_dir = home.join(".local").join("bin");
        let binary_path = install_dir.join("nostube-transcode");

        let log_dir = data_dir.join("logs");

        Self {
            env_file: data_dir.join("env"),
            identity_file: data_dir.join("identity.key"),
            pid_file: data_dir.join("nostube-transcode.pid"),
            stdout_log: log_dir.join("stdout.log"),
            stderr_log: log_dir.join("stderr.log"),
            log_dir,
            install_dir,
            binary_path,
            systemd_user_unit: config_base
                .join("systemd")
                .join("user")
                .join("nostube-transcode.service"),
            launchd_plist: home
                .join("Library")
                .join("LaunchAgents")
                .join("com.nostube.transcode.plist"),
            sysv_script: data_dir.join("nostube-transcode.initd"),
            data_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_paths_uses_data_dir_env() {
        env::set_var("DATA_DIR", "/tmp/test-nostube");
        let p = Paths::resolve();
        assert_eq!(p.data_dir, PathBuf::from("/tmp/test-nostube"));
        assert_eq!(p.env_file, PathBuf::from("/tmp/test-nostube/env"));
        assert_eq!(p.identity_file, PathBuf::from("/tmp/test-nostube/identity.key"));
        assert_eq!(p.pid_file, PathBuf::from("/tmp/test-nostube/nostube-transcode.pid"));
        assert_eq!(p.log_dir, PathBuf::from("/tmp/test-nostube/logs"));
        env::remove_var("DATA_DIR");
    }

    #[test]
    fn test_paths_systemd_user_unit() {
        env::set_var("HOME", "/home/testuser");
        env::remove_var("DATA_DIR");
        env::remove_var("XDG_DATA_HOME");
        env::remove_var("XDG_CONFIG_HOME");
        let p = Paths::resolve();
        assert_eq!(
            p.systemd_user_unit,
            PathBuf::from("/home/testuser/.config/systemd/user/nostube-transcode.service")
        );
        assert_eq!(
            p.launchd_plist,
            PathBuf::from("/home/testuser/Library/LaunchAgents/com.nostube.transcode.plist")
        );
    }

    #[test]
    fn test_paths_install_dir_default() {
        env::set_var("HOME", "/home/testuser");
        let p = Paths::resolve();
        assert_eq!(p.install_dir, PathBuf::from("/home/testuser/.local/bin"));
        assert_eq!(
            p.binary_path,
            PathBuf::from("/home/testuser/.local/bin/nostube-transcode")
        );
    }
}
