//! SysV init script generation and install guidance.

use anyhow::Result;
use tracing::info;

/// Generate a SysV init script for nostube-transcode.
pub fn generate_script(binary_path: &str, env_file: &str, run_user: &str) -> String {
    format!(
        r#"#!/bin/sh
### BEGIN INIT INFO
# Provides:          nostube-transcode
# Required-Start:    $network $remote_fs
# Required-Stop:     $network $remote_fs
# Default-Start:     2 3 4 5
# Default-Stop:      0 1 6
# Short-Description: nostube-transcode DVM
# Description:       Nostr Data Vending Machine for video transcoding
### END INIT INFO

DAEMON="{binary_path}"
DAEMON_USER="{run_user}"
PIDFILE="/var/run/nostube-transcode.pid"
ENV_FILE="{env_file}"

if [ -f "$ENV_FILE" ]; then
  set -a
  . "$ENV_FILE"
  set +a
fi

case "$1" in
  start)
    echo "Starting nostube-transcode..."
    if [ -f "$PIDFILE" ] && kill -0 $(cat "$PIDFILE") 2>/dev/null; then
      echo "Already running (pid $(cat "$PIDFILE"))"
      exit 0
    fi
    start-stop-daemon --start --background --make-pidfile --pidfile "$PIDFILE" \
      --chuid "$DAEMON_USER" --exec "$DAEMON" -- run
    echo "Started."
    ;;
  stop)
    echo "Stopping nostube-transcode..."
    if [ -f "$PIDFILE" ]; then
      start-stop-daemon --stop --pidfile "$PIDFILE" --retry 10
      rm -f "$PIDFILE"
      echo "Stopped."
    else
      echo "Not running."
    fi
    ;;
  restart)
    $0 stop
    sleep 1
    $0 start
    ;;
  status)
    if [ -f "$PIDFILE" ] && kill -0 $(cat "$PIDFILE") 2>/dev/null; then
      echo "nostube-transcode is running (pid $(cat "$PIDFILE"))"
    else
      echo "nostube-transcode is not running"
      exit 1
    fi
    ;;
  *)
    echo "Usage: $0 {{start|stop|restart|status}}"
    exit 1
    ;;
esac
"#
    )
}

/// Write SysV init script to data dir and print manual install instructions.
/// Does not install to /etc/init.d — that requires root and must be done manually.
pub fn write_script(paths: &crate::paths::Paths) -> Result<()> {
    let user = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string());
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();

    let script = generate_script(&binary, &env_file, &user);
    std::fs::write(&paths.sysv_script, &script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&paths.sysv_script)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&paths.sysv_script, perms)?;
    }

    info!("Wrote SysV init script: {:?}", paths.sysv_script);

    println!();
    println!(
        "SysV init script written to {}",
        paths.sysv_script.display()
    );
    println!("To install as a system service:");
    println!(
        "  sudo cp {} /etc/init.d/nostube-transcode",
        paths.sysv_script.display()
    );
    println!("  sudo update-rc.d nostube-transcode defaults");
    println!("  sudo service nostube-transcode start");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sysv_script_content() {
        let binary = "/home/alice/.local/bin/nostube-transcode";
        let env_file = "/home/alice/.local/share/nostube-transcode/env";
        let user = "alice";
        let script = generate_script(binary, env_file, user);
        assert!(script.contains("### BEGIN INIT INFO"));
        assert!(script.contains("nostube-transcode"));
        assert!(script.contains(binary));
        assert!(script.contains(env_file));
        assert!(script.contains("start-stop-daemon"));
        assert!(script.contains("status)"));
    }
}
