//! Prerequisite and configuration checks for `nostube-transcode doctor`.

use crate::paths::Paths;
use crate::service::ServiceManager;
use nostr_sdk::ToBech32;

pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(PartialEq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Ok,
            detail: detail.into(),
        }
    }
    fn warn(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warning,
            detail: detail.into(),
        }
    }
    fn err(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Error,
            detail: detail.into(),
        }
    }
}

/// Run all checks and return results.
pub fn run_checks(paths: &Paths) -> Vec<Check> {
    let mut checks = Vec::new();

    // Binary version
    let current_exe = std::env::current_exe().unwrap_or_default();
    checks.push(Check::ok(
        "binary",
        format!(
            "{} v{}",
            current_exe.display(),
            env!("CARGO_PKG_VERSION")
        ),
    ));

    // Data dir
    if paths.data_dir.exists() {
        checks.push(Check::ok(
            "data_dir",
            format!("{}", paths.data_dir.display()),
        ));
    } else {
        checks.push(Check::warn(
            "data_dir",
            format!(
                "{} (missing — run: nostube-transcode setup)",
                paths.data_dir.display()
            ),
        ));
    }

    // Env file
    if paths.env_file.exists() {
        checks.push(Check::ok(
            "env_file",
            format!("{}", paths.env_file.display()),
        ));
    } else {
        checks.push(Check::err(
            "env_file",
            format!(
                "{} missing — run: nostube-transcode setup",
                paths.env_file.display()
            ),
        ));
    }

    // OPERATOR_NPUB (check env file, then env var)
    let npub_val = std::env::var("OPERATOR_NPUB").ok().or_else(|| {
        if paths.env_file.exists() {
            crate::setup::read_env_file(&paths.env_file)
                .ok()
                .and_then(|m| m.get("OPERATOR_NPUB").cloned())
        } else {
            None
        }
    });

    match npub_val {
        Some(v) => match crate::setup::validate_operator_npub(&v) {
            Ok(pk) => checks.push(Check::ok(
                "operator_npub",
                pk.to_bech32().unwrap_or(v),
            )),
            Err(e) => checks.push(Check::err("operator_npub", format!("invalid: {e}"))),
        },
        None => checks.push(Check::err(
            "operator_npub",
            "missing — run: nostube-transcode setup",
        )),
    }

    // Identity key
    if paths.identity_file.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&paths.identity_file) {
                let mode = meta.permissions().mode() & 0o777;
                if mode == 0o600 {
                    checks.push(Check::ok("identity_key", "present (mode 0600)"));
                } else {
                    checks.push(Check::warn(
                        "identity_key",
                        format!("present but mode is {:o} (should be 0600)", mode),
                    ));
                }
            }
        }
        #[cfg(not(unix))]
        checks.push(Check::ok("identity_key", "present"));
    } else {
        checks.push(Check::warn(
            "identity_key",
            "not yet generated — will be created on first run",
        ));
    }

    // FFmpeg
    match crate::util::ffmpeg_discovery::FfmpegPaths::discover() {
        Ok(ff) => {
            checks.push(Check::ok("ffmpeg", format!("{}", ff.ffmpeg.display())));
            checks.push(Check::ok("ffprobe", format!("{}", ff.ffprobe.display())));
        }
        Err(e) => {
            checks.push(Check::err("ffmpeg", format!("not found: {e}")));
        }
    }

    // Service
    let mgr = ServiceManager::detect();
    let svc_path = match mgr {
        ServiceManager::SystemdUser => Some(paths.systemd_user_unit.as_path()),
        ServiceManager::Launchd => Some(paths.launchd_plist.as_path()),
        ServiceManager::SysV => Some(paths.sysv_script.as_path()),
        _ => None,
    };

    if let Some(svc) = svc_path {
        if svc.exists() {
            checks.push(Check::ok(
                "service",
                format!("{} ({})", mgr.name(), svc.display()),
            ));
        } else {
            checks.push(Check::warn(
                "service",
                format!("not installed — run: nostube-transcode install"),
            ));
        }
    } else {
        checks.push(Check::warn(
            "service",
            format!("{} — run in foreground: nostube-transcode run", mgr.name()),
        ));
    }

    // Process
    if crate::service::process::is_process_running(&paths.pid_file) {
        let pid =
            crate::service::process::read_pid_file(&paths.pid_file).unwrap_or(0);
        checks.push(Check::ok("process", format!("running (pid {pid})")));
    } else {
        checks.push(Check::warn(
            "process",
            "not running — run: nostube-transcode start",
        ));
    }

    checks
}

/// Print checks to stdout and return exit code.
/// 0 = all ok, 1 = at least one error, 2 = warnings only.
pub fn print_and_exit_code(checks: &[Check]) -> i32 {
    let mut has_error = false;
    let mut has_warning = false;

    for c in checks {
        let symbol = match c.status {
            CheckStatus::Ok => "✓",
            CheckStatus::Warning => "⚠",
            CheckStatus::Error => "✗",
        };
        println!("  {} {}: {}", symbol, c.name, c.detail);
        if c.status == CheckStatus::Error {
            has_error = true;
        }
        if c.status == CheckStatus::Warning {
            has_warning = true;
        }
    }

    if has_error {
        1
    } else if has_warning {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_and_exit_code_all_ok() {
        let checks = vec![
            Check::ok("a", "good"),
            Check::ok("b", "also good"),
        ];
        assert_eq!(print_and_exit_code(&checks), 0);
    }

    #[test]
    fn test_print_and_exit_code_warnings_only() {
        let checks = vec![
            Check::ok("a", "good"),
            Check::warn("b", "meh"),
        ];
        assert_eq!(print_and_exit_code(&checks), 2);
    }

    #[test]
    fn test_print_and_exit_code_has_error() {
        let checks = vec![
            Check::ok("a", "good"),
            Check::err("b", "broken"),
        ];
        assert_eq!(print_and_exit_code(&checks), 1);
    }
}
