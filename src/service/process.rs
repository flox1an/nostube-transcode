//! PID file management for foreground/service-manager fallback.

use std::path::Path;
use tracing::warn;

/// Write the current process PID to a file. Creates parent dirs as needed.
pub fn write_pid_file(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pid = std::process::id().to_string();
    if let Err(e) = std::fs::write(path, &pid) {
        warn!("Failed to write PID file {:?}: {}", path, e);
    }
}

/// Read PID from file. Returns None if file is missing or unparseable.
pub fn read_pid_file(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Returns true if the PID recorded in the file corresponds to a running process.
pub fn is_process_running(pid_path: &Path) -> bool {
    let Some(pid) = read_pid_file(pid_path) else {
        return false;
    };
    is_pid_alive(pid)
}

/// Send signal 0 to check if process is alive (Unix only).
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
        ret == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Kill the process recorded in the PID file (SIGTERM, then wait briefly).
pub fn kill_existing_pid() {
    #[cfg(unix)]
    {
        let paths = crate::paths::Paths::resolve();
        if let Some(pid) = read_pid_file(&paths.pid_file) {
            if is_pid_alive(pid) {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !is_pid_alive(pid) {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_read_pid_file() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");
        write_pid_file(&pid_path);
        let pid = read_pid_file(&pid_path);
        assert!(pid.is_some());
        let current_pid = std::process::id();
        assert_eq!(pid.unwrap(), current_pid);
    }

    #[test]
    fn test_read_missing_pid_file() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("missing.pid");
        let pid = read_pid_file(&pid_path);
        assert!(pid.is_none());
    }

    #[test]
    fn test_is_process_running_missing_file() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("missing.pid");
        assert!(!is_process_running(&pid_path));
    }

    #[test]
    fn test_current_process_is_running() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("current.pid");
        write_pid_file(&pid_path);
        assert!(is_process_running(&pid_path));
    }
}
