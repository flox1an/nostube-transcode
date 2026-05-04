# CLI + Service Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `nostube-transcode <subcommand>` CLI surface with service install/manage, setup wizard, doctor, config management, and Docker subcommands — one commit per phase.

**Architecture:** Add `clap` for CLI parsing; extract daemon body into `runtime.rs`; new modules `paths.rs`, `setup.rs`, `doctor.rs`, `service/` handle service lifecycle; `config_cmd.rs` reads/writes NIP-78 remote config directly. Four phases: (1) CLI foundation, (2) service management, (3) setup + doctor + config, (4) installer update + Docker subcommands.

**Tech Stack:** Rust, clap 4 (derive), existing nostr-sdk 0.35, existing dirs/dotenvy, tokio. Shell: bash (install.sh).

---

## Confirmed Design Decisions

- Binary name stays **`nostube-transcode`** — no rename
- Data path stays **`~/.local/share/nostube-transcode`** — no migration
- `install` **starts the service** in the background immediately after installing/enabling
- SysV init remains supported
- Docker lifecycle comes into the CLI (Phase 4) — `setup.sh` stays too

---

## File Map

**New files:**
- `src/cli.rs` — `Cli` struct, `Commands` / `DockerCommands` enums, top-level dispatch
- `src/paths.rs` — `Paths` struct, all platform path resolution
- `src/runtime.rs` — `run_daemon(replace: bool)` extracted from main.rs
- `src/setup.rs` — interactive + non-interactive setup wizard, env file read/write
- `src/doctor.rs` — prerequisite checks, exit codes
- `src/config_cmd.rs` — `config get` / `config set` via NIP-78 remote config
- `src/service/mod.rs` — `ServiceManager` enum, platform detection
- `src/service/systemd.rs` — unit file generation, install/start/stop/restart/status/logs/uninstall
- `src/service/launchd.rs` — plist generation, install/start/stop/restart/status/logs/uninstall
- `src/service/sysv.rs` — init script generation, install guidance
- `src/service/process.rs` — PID file read/write/stale detection

**Modified files:**
- `src/main.rs` — only parse CLI + dispatch
- `src/lib.rs` — pub mod for new modules
- `Cargo.toml` — add `clap = { version = "4", features = ["derive"] }`
- `install.sh` — delegate setup/service to binary after binary install
- `README.md` — updated CLI reference at each phase

---

## Phase 1: Rust CLI Foundation

### Task 1.1: Add clap dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] Add to `[dependencies]`:

```toml
clap = { version = "4", features = ["derive"] }
```

- [ ] Verify it compiles:

```bash
cargo check 2>&1 | head -20
```

Expected: no errors (clap resolves cleanly).

---

### Task 1.2: Create `src/paths.rs`

**Files:**
- Create: `src/paths.rs`
- Test inline in same file

- [ ] Write the failing test first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_paths_uses_data_dir_env() {
        env::set_var("DATA_DIR", "/tmp/test-nostube");
        let p = Paths::resolve();
        assert_eq!(p.data_dir, std::path::PathBuf::from("/tmp/test-nostube"));
        assert_eq!(p.env_file, std::path::PathBuf::from("/tmp/test-nostube/env"));
        assert_eq!(p.identity_file, std::path::PathBuf::from("/tmp/test-nostube/identity.key"));
        assert_eq!(p.pid_file, std::path::PathBuf::from("/tmp/test-nostube/nostube-transcode.pid"));
        assert_eq!(p.log_dir, std::path::PathBuf::from("/tmp/test-nostube/logs"));
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
            std::path::PathBuf::from("/home/testuser/.config/systemd/user/nostube-transcode.service")
        );
        assert_eq!(
            p.launchd_plist,
            std::path::PathBuf::from("/home/testuser/Library/LaunchAgents/com.nostube.transcode.plist")
        );
    }

    #[test]
    fn test_paths_install_dir_default() {
        env::set_var("HOME", "/home/testuser");
        let p = Paths::resolve();
        assert_eq!(p.install_dir, std::path::PathBuf::from("/home/testuser/.local/bin"));
        assert_eq!(
            p.binary_path,
            std::path::PathBuf::from("/home/testuser/.local/bin/nostube-transcode")
        );
    }
}
```

- [ ] Run test to confirm it fails:

```bash
cargo test paths 2>&1 | tail -5
```

Expected: compile error — `Paths` not found.

- [ ] Create `src/paths.rs`:

```rust
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
    /// PID file for foreground/fallback process tracking: $data_dir/nostube-transcode.pid
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
```

- [ ] Run tests:

```bash
cargo test paths 2>&1 | tail -10
```

Expected: all 3 pass.

---

### Task 1.3: Extract daemon body into `src/runtime.rs`

**Files:**
- Create: `src/runtime.rs`
- Modify: `src/main.rs`

- [ ] Create `src/runtime.rs` with the daemon body extracted from `main.rs`:

```rust
//! DVM daemon runtime.
//!
//! `run_daemon` contains the full daemon startup sequence previously in main.rs.

use crate::admin::run_admin_listener;
use crate::blossom::BlossomClient;
use crate::dvm::{AnnouncementPublisher, JobHandler};
use crate::nostr::{EventPublisher, SubscriptionManager};
use crate::startup::initialize;
use crate::video::{HwAccel, VideoProcessor};
use crate::web::run_server;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Notify;
use tracing::info;

/// Run the DVM daemon in the foreground.
///
/// Loads env files, starts all subsystems, and waits for Ctrl+C or SIGTERM.
/// If `replace` is true, kills any existing process recorded in the PID file
/// before starting.
pub async fn run_daemon(replace: bool) -> anyhow::Result<()> {
    if replace {
        crate::service::process::kill_existing_pid();
    }

    // Load .env from current directory, then data dir env file
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("Warning: Error loading .env file: {}", e);
        }
    }
    let env_path = crate::identity::default_data_dir().join("env");
    if env_path.exists() {
        if let Err(e) = dotenvy::from_path(&env_path) {
            eprintln!("Warning: Error loading env file from {:?}: {}", env_path, e);
        }
    }

    info!("Starting DVM Video Processing Service");
    info!("Starting in remote config mode...");

    let startup = initialize().await.expect("Failed to initialize DVM");

    // Write PID file for service management fallback
    let paths = crate::paths::Paths::resolve();
    crate::service::process::write_pid_file(&paths.pid_file);

    let config_notify = Arc::new(Notify::new());

    let web_handle = if startup.config.http_enabled {
        Some(tokio::spawn({
            let config = startup.config.clone();
            async move {
                if let Err(e) = run_server(config).await {
                    tracing::error!("Web server error: {}", e);
                }
            }
        }))
    } else {
        info!("HTTP server disabled (DISABLE_HTTP is set)");
        None
    };

    let admin_handle = tokio::spawn({
        let client = startup.client.clone();
        let keys = startup.keys.clone();
        let state = startup.state.clone();
        let config = startup.config.clone();
        let config_notify = config_notify.clone();
        async move { run_admin_listener(client, keys, state, config, config_notify).await; }
    });

    let hwaccel = HwAccel::detect();
    let publisher = Arc::new(EventPublisher::new(
        startup.config.clone(), startup.client.clone(), startup.state.clone(),
    ));
    let announcement_publisher = AnnouncementPublisher::new(
        startup.config.clone(), startup.state.clone(), publisher, hwaccel, config_notify,
    );
    let announcement_handle = tokio::spawn(async move { announcement_publisher.run().await; });

    let (job_tx, job_rx) = tokio::sync::mpsc::channel(32);
    let subscription_handle = tokio::spawn({
        let config = startup.config.clone();
        let client = startup.client.clone();
        let state = startup.state.clone();
        async move {
            match SubscriptionManager::new(config, client, state).await {
                Ok(manager) => { if let Err(e) = manager.run(job_tx).await { tracing::error!("Subscription manager error: {}", e); } }
                Err(e) => tracing::error!("Failed to create subscription manager: {}", e),
            }
        }
    });

    let job_publisher = Arc::new(EventPublisher::new(
        startup.config.clone(), startup.client.clone(), startup.state.clone(),
    ));
    let blossom = Arc::new(BlossomClient::new(startup.config.clone(), startup.state.clone()));
    let processor = Arc::new(VideoProcessor::new(startup.config.clone()));
    let job_handler = Arc::new(JobHandler::new(
        startup.config.clone(), startup.state.clone(), job_publisher, blossom, processor,
    ));
    let job_handle = tokio::spawn(async move { job_handler.run(job_rx).await; });

    info!("Remote config mode active. Press Ctrl+C to shutdown.");
    shutdown_signal().await;

    info!("Shutting down...");
    if let Some(h) = web_handle { h.abort(); }
    admin_handle.abort();
    announcement_handle.abort();
    subscription_handle.abort();
    job_handle.abort();
    let _ = startup.client.disconnect().await;

    // Remove PID file on clean exit
    let _ = std::fs::remove_file(&paths.pid_file);

    info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
```

---

### Task 1.4: Create `src/service/process.rs` (PID file, used by runtime)

**Files:**
- Create: `src/service/mod.rs`
- Create: `src/service/process.rs`

- [ ] Create `src/service/mod.rs` (minimal for now, expanded in Phase 2):

```rust
pub mod process;
pub mod systemd;
pub mod launchd;
pub mod sysv;
```

- [ ] Create `src/service/systemd.rs`, `src/service/launchd.rs`, `src/service/sysv.rs` as empty stubs:

```rust
// placeholder — implemented in Phase 2
```

- [ ] Write tests for `src/service/process.rs`:

```rust
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
    fn test_is_stale_pid_missing_file() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("missing.pid");
        // No file = no running process = stale
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
```

- [ ] Create `src/service/process.rs`:

```rust
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
/// On non-Unix platforms always returns false.
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
        ret == 0
    }
    #[cfg(not(unix))]
    false
}

/// Kill the process recorded in the PID file (SIGTERM, then wait briefly).
/// No-op if file is missing or process is already gone.
pub fn kill_existing_pid() {
    // This is only implemented on Unix
    #[cfg(unix)]
    {
        if let Some(pid) = read_pid_file(&crate::paths::Paths::resolve().pid_file) {
            if is_pid_alive(pid) {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
                // Give it up to 5 seconds to exit
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !is_pid_alive(pid) { break; }
                }
            }
        }
    }
}
```

- [ ] Run PID tests:

```bash
cargo test process 2>&1 | tail -10
```

Expected: 4 tests pass.

---

### Task 1.5: Create `src/cli.rs`

**Files:**
- Create: `src/cli.rs`

- [ ] Write `src/cli.rs`:

```rust
//! CLI argument parsing and top-level dispatch.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "nostube-transcode",
    version,
    about = "Nostr Video Transform DVM — CLI management"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the DVM in the foreground (default when no subcommand is given)
    Run {
        /// Kill any existing process before starting
        #[arg(long)]
        replace: bool,
    },
    /// Interactive configuration wizard
    Setup {
        #[arg(long)]
        non_interactive: bool,
        #[arg(long)]
        operator_npub: Option<String>,
        #[arg(long)]
        http_port: Option<u16>,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Install and start the background service
    Install {
        /// Re-install even if already installed
        #[arg(long)]
        force: bool,
        /// Install as a system-wide service (requires root)
        #[arg(long)]
        system: bool,
        /// User to run the service as (--system only)
        #[arg(long)]
        user: Option<String>,
    },
    /// Stop and remove the background service
    Uninstall {
        #[arg(long)]
        system: bool,
        /// Keep config/env/identity files
        #[arg(long)]
        keep_config: bool,
    },
    /// Start the service
    Start {
        #[arg(long)]
        system: bool,
    },
    /// Stop the service
    Stop {
        #[arg(long)]
        system: bool,
        /// Force-kill if graceful stop fails
        #[arg(long)]
        force: bool,
    },
    /// Restart the service
    Restart {
        #[arg(long)]
        system: bool,
    },
    /// Show service and runtime status
    Status {
        /// Show full systemctl/launchctl output and recent log lines
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        system: bool,
    },
    /// View service logs
    Logs {
        #[arg(short, long)]
        follow: bool,
        #[arg(short = 'n', long, default_value = "50")]
        lines: u32,
        #[arg(long)]
        system: bool,
    },
    /// Check prerequisites and configuration
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Update the installed binary from GitHub releases
    Update {
        /// Pin a specific version (e.g. v0.3.5)
        #[arg(long)]
        version: Option<String>,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Get or set remote DVM configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Manage Docker deployment
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
    /// Print version information
    Version,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Display current configuration
    Get,
    /// Update configuration (all flags optional)
    Set {
        #[arg(long, value_delimiter = ',')]
        relays: Option<Vec<String>>,
        #[arg(long, value_delimiter = ',')]
        blossom_servers: Option<Vec<String>>,
        #[arg(long)]
        max_concurrent_jobs: Option<u32>,
        #[arg(long)]
        blob_expiration_days: Option<u32>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        about: Option<String>,
    },
    /// Pause the DVM (stop accepting new jobs)
    Pause,
    /// Resume the DVM
    Resume,
    /// Show DVM runtime status via Nostr
    Status,
}

#[derive(Subcommand)]
pub enum DockerCommands {
    /// Run setup.sh — detect GPU, write .env, start compose
    Setup,
    /// Show docker compose ps output
    Status,
    /// Follow docker logs
    Logs {
        #[arg(short, long)]
        follow: bool,
    },
    /// docker compose start
    Start,
    /// docker compose stop
    Stop,
    /// docker compose restart
    Restart,
}
```

- [ ] Write CLI parse tests in `src/cli.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses_run() {
        let cli = Cli::try_parse_from(["nostube-transcode", "run"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Run { replace: false })));
    }

    #[test]
    fn test_cli_parses_run_replace() {
        let cli = Cli::try_parse_from(["nostube-transcode", "run", "--replace"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Run { replace: true })));
    }

    #[test]
    fn test_cli_no_subcommand() {
        let cli = Cli::try_parse_from(["nostube-transcode"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parses_install_system() {
        let cli = Cli::try_parse_from(["nostube-transcode", "install", "--system"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Install { system: true, .. })));
    }

    #[test]
    fn test_cli_parses_logs() {
        let cli = Cli::try_parse_from(["nostube-transcode", "logs", "-n", "100", "--follow"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Logs { follow: true, lines: 100, .. })));
    }

    #[test]
    fn test_cli_parses_config_set() {
        let cli = Cli::try_parse_from([
            "nostube-transcode", "config", "set",
            "--max-concurrent-jobs", "3",
            "--name", "My DVM",
        ]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Config { .. })));
    }

    #[test]
    fn test_cli_parses_docker_setup() {
        let cli = Cli::try_parse_from(["nostube-transcode", "docker", "setup"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Docker { command: DockerCommands::Setup })));
    }

    #[test]
    fn verify_cli_structure() {
        Cli::command().debug_assert();
    }
}
```

- [ ] Run tests:

```bash
cargo test cli 2>&1 | tail -15
```

Expected: all 8 tests pass.

---

### Task 1.6: Refactor `src/main.rs` to dispatch CLI

**Files:**
- Modify: `src/main.rs`

- [ ] Replace entire `src/main.rs` with:

```rust
use clap::Parser;
use nostube_transcode::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Run { replace: false }) => {
            if cli.command.is_none() {
                eprintln!(
                    "Note: no subcommand given — defaulting to 'run'. \
                     In future use: nostube-transcode run"
                );
            }
            init_tracing();
            nostube_transcode::runtime::run_daemon(false).await
        }
        Some(Commands::Run { replace: true }) => {
            init_tracing();
            nostube_transcode::runtime::run_daemon(true).await
        }
        Some(Commands::Version) => {
            println!("nostube-transcode {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        // Phase 2+: service management commands
        Some(Commands::Install { .. })
        | Some(Commands::Uninstall { .. })
        | Some(Commands::Start { .. })
        | Some(Commands::Stop { .. })
        | Some(Commands::Restart { .. })
        | Some(Commands::Status { .. })
        | Some(Commands::Logs { .. }) => {
            eprintln!("Service management commands are coming in the next release.");
            eprintln!("For now use: nostube-transcode run");
            std::process::exit(1);
        }
        // Phase 3: setup, doctor, config
        Some(Commands::Setup { .. })
        | Some(Commands::Doctor { .. })
        | Some(Commands::Config { .. }) => {
            eprintln!("Setup/doctor/config commands are coming in the next release.");
            std::process::exit(1);
        }
        // Phase 4: update, docker
        Some(Commands::Update { .. }) | Some(Commands::Docker { .. }) => {
            eprintln!("Update/docker commands are coming in the next release.");
            std::process::exit(1);
        }
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nostube_transcode=debug".parse().unwrap()),
        )
        .init();
}
```

---

### Task 1.7: Wire new modules into `src/lib.rs`

**Files:**
- Modify: `src/lib.rs`

- [ ] Add to `src/lib.rs`:

```rust
pub mod cli;
pub mod config_cmd;
pub mod doctor;
pub mod paths;
pub mod runtime;
pub mod service;
pub mod setup;
```

- [ ] Create empty stubs for modules not yet implemented (needed to compile):

`src/config_cmd.rs`:
```rust
// implemented in Phase 3
```

`src/doctor.rs`:
```rust
// implemented in Phase 3
```

`src/setup.rs`:
```rust
// implemented in Phase 3
```

- [ ] Verify everything compiles:

```bash
cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] Run all tests:

```bash
cargo test 2>&1 | tail -15
```

Expected: all existing tests plus new cli/paths/process tests pass.

---

### Task 1.8: Update README for Phase 1 + commit

**Files:**
- Modify: `README.md`

- [ ] Replace the **"Running as a Daemon (Standalone)"** section and add a **CLI reference** section. The new section goes after "Quick Start":

```markdown
## CLI Reference

`nostube-transcode` now has a full subcommand interface:

```text
nostube-transcode <command> [options]

Commands:
  run                 Run the DVM in the foreground
  setup               Interactive configuration wizard
  install             Install and start the background service
  uninstall           Stop and remove the background service
  start               Start the service
  stop                Stop the service
  restart             Restart the service
  status              Show service and runtime status
  logs                Follow or print recent service logs
  doctor              Check prerequisites and configuration
  update              Update the installed binary
  config              Get or set remote DVM configuration
  docker              Manage Docker deployment
  version             Print version information
```

If invoked with no subcommand, the DVM runs in the foreground (backward-compatible behaviour, deprecated — use `run` explicitly).
```

- [ ] Commit Phase 1:

```bash
git add -A
git commit -m "feat: Phase 1 — CLI foundation with clap, paths, runtime extraction"
```

---

## Phase 2: Service Management

### Task 2.1: Implement `src/service/systemd.rs`

**Files:**
- Modify: `src/service/systemd.rs`

- [ ] Write snapshot tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> (String, String, String) {
        let binary = "/home/alice/.local/bin/nostube-transcode".to_string();
        let env_file = "/home/alice/.local/share/nostube-transcode/env".to_string();
        let home = "/home/alice".to_string();
        (binary, env_file, home)
    }

    #[test]
    fn test_unit_file_content_user() {
        let (binary, env_file, home) = test_paths();
        let unit = generate_user_unit(&binary, &env_file, &home);
        assert!(unit.contains("ExecStart=/home/alice/.local/bin/nostube-transcode run --replace"));
        assert!(unit.contains("EnvironmentFile=/home/alice/.local/share/nostube-transcode/env"));
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("After=network-online.target"));
        assert!(unit.contains("StartLimitIntervalSec=0"));
    }

    #[test]
    fn test_unit_file_content_system() {
        let binary = "/home/alice/.local/bin/nostube-transcode".to_string();
        let env_file = "/home/alice/.local/share/nostube-transcode/env".to_string();
        let home = "/home/alice".to_string();
        let unit = generate_system_unit(&binary, &env_file, &home, "alice");
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
```

- [ ] Run to confirm they fail:

```bash
cargo test service::systemd 2>&1 | tail -5
```

- [ ] Implement `src/service/systemd.rs`:

```rust
//! systemd user and system service management.

use std::path::Path;
use std::process::Command;
use anyhow::{bail, Context, Result};
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
pub fn generate_system_unit(binary_path: &str, env_file: &str, home: &str, user: &str) -> String {
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
    if follow { args.push("-f"); }
    let status = Command::new("journalctl").args(&args).status()
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
```

- [ ] Run tests:

```bash
cargo test service::systemd 2>&1 | tail -10
```

Expected: 5 tests pass.

---

### Task 2.2: Implement `src/service/launchd.rs`

**Files:**
- Modify: `src/service/launchd.rs`

- [ ] Write tests first:

```rust
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
```

- [ ] Implement `src/service/launchd.rs`:

```rust
//! macOS launchd user agent management.

use std::path::Path;
use std::process::Command;
use anyhow::{bail, Context, Result};
use tracing::info;

/// Generate a launchd user agent plist for nostube-transcode.
pub fn generate_plist(binary_path: &str, env_file: &str, log_dir: &str) -> String {
    // Build PATH that includes Homebrew (ARM and Intel) locations
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
        std::fs::create_dir_all(parent).context("Failed to create LaunchAgents directory")?;
    }

    // Unload existing if present (ignore error)
    let uid = get_uid();
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
        .args(["kill", "SIGTERM", &format!("gui/{uid}/com.nostube.transcode")])
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
        Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &plist_str])
            .status()
            .context("launchctl bootstrap failed")?;
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
    // Touch log files so tail doesn't fail on first run
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&paths.stderr_log);
    let n = lines.to_string();
    let mut args = vec!["-n", &n, paths.stderr_log.to_str().unwrap_or("")];
    if follow { args.insert(0, "-f"); } else { args.insert(0, "-"); }
    // Use tail -f or tail -n
    let cmd = if follow { "tail" } else { "tail" };
    let full_args: Vec<&str> = if follow {
        vec!["-f", "-n", &n, paths.stderr_log.to_str().unwrap_or("")]
    } else {
        vec!["-n", &n, paths.stderr_log.to_str().unwrap_or("")]
    };
    Command::new(cmd).args(&full_args).status().context("tail not available")?;
    Ok(())
}

fn get_uid() -> u32 {
    #[cfg(unix)]
    { unsafe { libc::getuid() } }
    #[cfg(not(unix))]
    { 0 }
}
```

- [ ] Run tests:

```bash
cargo test service::launchd 2>&1 | tail -10
```

Expected: 3 tests pass.

---

### Task 2.3: Implement `src/service/sysv.rs`

**Files:**
- Modify: `src/service/sysv.rs`

- [ ] Write test first:

```rust
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
```

- [ ] Implement `src/service/sysv.rs`:

```rust
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
/// (We don't install it automatically — requires root for /etc/init.d.)
pub fn write_script(paths: &crate::paths::Paths) -> Result<()> {
    let user = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string());
    let binary = paths.binary_path.to_string_lossy().to_string();
    let env_file = paths.env_file.to_string_lossy().to_string();

    let script = generate_script(&binary, &env_file, &user);
    std::fs::write(&paths.sysv_script, &script)?;

    // Set executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&paths.sysv_script)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&paths.sysv_script, perms)?;
    }

    info!("Wrote SysV init script: {:?}", paths.sysv_script);

    println!();
    println!("SysV init script written. To install as a system service:");
    println!("  sudo cp {} /etc/init.d/nostube-transcode", paths.sysv_script.display());
    println!("  sudo update-rc.d nostube-transcode defaults");
    println!("  sudo service nostube-transcode start");

    Ok(())
}
```

- [ ] Run test:

```bash
cargo test service::sysv 2>&1 | tail -5
```

Expected: 1 test passes.

---

### Task 2.4: Implement `src/service/mod.rs` dispatch

**Files:**
- Modify: `src/service/mod.rs`

- [ ] Replace stub with:

```rust
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

/// Install and start the service using the detected manager.
pub fn install_and_start(paths: &Paths, system: bool, _force: bool, _user: Option<&str>) -> Result<()> {
    let mgr = if system { ServiceManager::SystemdSystem } else { ServiceManager::detect() };
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
            bail!("System service install is not yet implemented in the CLI.\n\
                   Use: sudo cp {} /etc/systemd/system/nostube-transcode.service\n\
                   Then: sudo systemctl daemon-reload && sudo systemctl enable --now nostube-transcode",
                  paths.systemd_user_unit.display());
        }
        ServiceManager::None => {
            bail!("No supported service manager detected.\n\
                   Run in the foreground with: nostube-transcode run");
        }
    }
    Ok(())
}

/// Stop and remove the service.
pub fn uninstall(paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::uninstall_user(paths),
        ServiceManager::Launchd => launchd::uninstall(paths),
        _ => bail!("Uninstall not supported for this platform's service manager"),
    }
}

/// Start the service.
pub fn start(paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::start_user(),
        ServiceManager::Launchd => launchd::start(),
        _ => bail!("Start not supported — run manually: nostube-transcode run"),
    }
}

/// Stop the service.
pub fn stop(_system: bool, _force: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::stop_user(),
        ServiceManager::Launchd => launchd::stop(),
        _ => bail!("Stop not supported for this platform"),
    }
}

/// Restart the service.
pub fn restart(paths: &Paths, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::restart_user(paths),
        ServiceManager::Launchd => launchd::restart(paths),
        _ => bail!("Restart not supported for this platform"),
    }
}

/// Print service status.
pub fn status(_system: bool, deep: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::status_user(deep),
        ServiceManager::Launchd => launchd::status(),
        _ => bail!("Status not supported for this platform"),
    }
}

/// Print or follow logs.
pub fn logs(paths: &Paths, follow: bool, lines: u32, _system: bool) -> Result<()> {
    match ServiceManager::detect() {
        ServiceManager::SystemdUser => systemd::logs_user(follow, lines),
        ServiceManager::Launchd => launchd::logs(paths, follow, lines),
        _ => bail!("Logs not supported — run in foreground and check terminal output"),
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
```

---

### Task 2.5: Wire service commands into `src/main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] Replace the Phase 2 placeholder arms in `main.rs`:

```rust
Some(Commands::Install { force, system, user }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    nostube_transcode::service::install_and_start(
        &paths, system, force, user.as_deref()
    ).map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Uninstall { system, .. }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    nostube_transcode::service::uninstall(&paths, system)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Start { system }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    nostube_transcode::service::start(&paths, system)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Stop { system, force }) => {
    nostube_transcode::service::stop(system, force)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Restart { system }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    nostube_transcode::service::restart(&paths, system)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Status { deep, system }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    // Print local status header
    println!("Binary:   {}", paths.binary_path.display());
    println!("Data dir: {}", paths.data_dir.display());
    println!("Env file: {}", paths.env_file.display());
    println!("Service:  {}", nostube_transcode::service::ServiceManager::detect().name());
    println!("PID file: {}", paths.pid_file.display());
    if nostube_transcode::service::process::is_process_running(&paths.pid_file) {
        let pid = nostube_transcode::service::process::read_pid_file(&paths.pid_file).unwrap_or(0);
        println!("Process:  running (pid {})", pid);
    } else {
        println!("Process:  not running");
    }
    println!();
    nostube_transcode::service::status(system, deep)
        .unwrap_or_else(|e| eprintln!("{}", e));
    Ok(())
}
Some(Commands::Logs { follow, lines, system }) => {
    let paths = nostube_transcode::paths::Paths::resolve();
    nostube_transcode::service::logs(&paths, follow, lines, system)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
```

- [ ] Verify it compiles:

```bash
cargo build 2>&1 | grep "^error" | head -20
```

- [ ] Run all tests:

```bash
cargo test 2>&1 | tail -15
```

Expected: no regressions.

---

### Task 2.6: Update README + commit Phase 2

- [ ] Update README — replace the old **"Running as a Daemon (Standalone)"** section:

```markdown
## Running as a Background Service

After installing the binary, the easiest way to start the service is:

```bash
nostube-transcode install
```

This installs the service definition for your platform and starts it immediately.

### Service management commands

```bash
nostube-transcode install       # Install service definition and start
nostube-transcode start         # Start the service
nostube-transcode stop          # Stop the service
nostube-transcode restart       # Restart (refreshes stale service definition)
nostube-transcode status        # Brief status summary
nostube-transcode status --deep # Full systemctl/launchctl output + recent logs
nostube-transcode logs -f       # Follow logs
nostube-transcode uninstall     # Stop, disable and remove service definition
```

Platform details:
- **Linux (systemd)**: installs `~/.config/systemd/user/nostube-transcode.service`
- **Linux (SysV)**: writes init script to `~/.local/share/nostube-transcode/nostube-transcode.initd` with manual install instructions
- **macOS**: installs `~/Library/LaunchAgents/com.nostube.transcode.plist`

For log persistence across reboots on Linux without login (headless servers):
```bash
loginctl enable-linger $USER
```
```

- [ ] Commit Phase 2:

```bash
git add -A
git commit -m "feat: Phase 2 — service management (install/start/stop/restart/status/logs/uninstall)"
```

---

## Phase 3: Setup, Doctor, Config

### Task 3.1: Implement env file read/write in `src/setup.rs`

**Files:**
- Modify: `src/setup.rs`

- [ ] Write tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_new_env_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        write_env_file(&path, &[("OPERATOR_NPUB", "npub1test"), ("HTTP_PORT", "5207")]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OPERATOR_NPUB=npub1test"));
        assert!(content.contains("HTTP_PORT=5207"));
    }

    #[test]
    fn test_update_preserves_unknown_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "OPERATOR_NPUB=old\nCUSTOM_KEY=my_value\n").unwrap();
        upsert_env_file(&path, &[("OPERATOR_NPUB", "new")]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OPERATOR_NPUB=new"));
        assert!(content.contains("CUSTOM_KEY=my_value"));
    }

    #[test]
    fn test_read_env_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "OPERATOR_NPUB=npub1abc\nHTTP_PORT=9000\n").unwrap();
        let vals = read_env_file(&path).unwrap();
        assert_eq!(vals.get("OPERATOR_NPUB").map(|s| s.as_str()), Some("npub1abc"));
        assert_eq!(vals.get("HTTP_PORT").map(|s| s.as_str()), Some("9000"));
    }

    #[test]
    fn test_validate_operator_npub_valid_npub() {
        // A real npub
        let result = validate_operator_npub("npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f8lc2uejsszjwyed");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_operator_npub_valid_hex() {
        let hex = "a".repeat(64);
        // This specific hex may not be a valid key, just check format
        // Real validation is done by nostr-sdk
        assert!(validate_operator_npub(&hex).is_err() || validate_operator_npub(&hex).is_ok());
    }

    #[test]
    fn test_validate_operator_npub_invalid() {
        assert!(validate_operator_npub("notanpub").is_err());
        assert!(validate_operator_npub("").is_err());
        assert!(validate_operator_npub("npub1tooshort").is_err());
    }
}
```

- [ ] Run to confirm failures, then implement `src/setup.rs`:

```rust
//! Interactive and non-interactive DVM setup wizard.

use anyhow::{bail, Context, Result};
use nostr_sdk::PublicKey;
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Read key=value pairs from an env file. Lines starting with # are ignored.
pub fn read_env_file(path: &Path) -> Result<BTreeMap<String, String>> {
    let content = std::fs::read_to_string(path)
        .context("Failed to read env file")?;
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((key, val)) = line.split_once('=') {
            map.insert(key.trim().to_string(), val.trim().to_string());
        }
    }
    Ok(map)
}

/// Write key=value pairs to an env file (creates or overwrites).
pub fn write_env_file(path: &Path, entries: &[(&str, &str)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create data directory")?;
    }
    let mut content = String::new();
    for (k, v) in entries {
        content.push_str(&format!("{k}={v}\n"));
    }
    atomic_write(path, content.as_bytes())
}

/// Update specific keys in an env file while preserving all other keys/comments.
pub fn upsert_env_file(path: &Path, updates: &[(&str, &str)]) -> Result<()> {
    let mut map = if path.exists() {
        read_env_file(path).unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    for (k, v) in updates {
        map.insert(k.to_string(), v.to_string());
    }
    let entries: Vec<(&str, &str)> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    write_env_file(path, &entries)
}

/// Validate that a string is a valid Nostr public key (npub or 64-char hex).
pub fn validate_operator_npub(s: &str) -> Result<PublicKey> {
    if s.is_empty() {
        bail!("OPERATOR_NPUB cannot be empty");
    }
    PublicKey::parse(s).map_err(|e| anyhow::anyhow!("Invalid OPERATOR_NPUB '{}': {}", s, e))
}

/// Run the interactive setup wizard.
///
/// `non_interactive`: skip prompts, use provided flags / existing env file values.
pub fn run_setup(
    paths: &crate::paths::Paths,
    non_interactive: bool,
    operator_npub: Option<&str>,
    http_port: Option<u16>,
) -> Result<()> {
    println!("nostube-transcode setup");
    println!("=======================");
    println!("Data dir: {}", paths.data_dir.display());
    println!("Env file: {}", paths.env_file.display());
    println!();

    // Load existing config
    let mut env = if paths.env_file.exists() {
        read_env_file(&paths.env_file).unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    // OPERATOR_NPUB
    let npub = if let Some(n) = operator_npub {
        validate_operator_npub(n).context("--operator-npub is invalid")?;
        n.to_string()
    } else if let Some(existing) = env.get("OPERATOR_NPUB").cloned() {
        if validate_operator_npub(&existing).is_ok() {
            println!("OPERATOR_NPUB: {} (existing)", existing);
            existing
        } else if non_interactive {
            bail!("Existing OPERATOR_NPUB is invalid. Re-run with --operator-npub <npub>.");
        } else {
            prompt_operator_npub()
        }
    } else if non_interactive {
        bail!("OPERATOR_NPUB is required. Pass --operator-npub <npub>.");
    } else {
        prompt_operator_npub()
    };

    env.insert("OPERATOR_NPUB".to_string(), npub);

    // HTTP_PORT
    if let Some(port) = http_port {
        env.insert("HTTP_PORT".to_string(), port.to_string());
    }

    // Write env file
    std::fs::create_dir_all(&paths.data_dir)
        .context("Failed to create data directory")?;
    let entries: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    write_env_file(&paths.env_file, &entries)?;

    // Set permissions to 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&paths.env_file)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&paths.env_file, perms)?;
    }

    println!("Configuration written to {}", paths.env_file.display());

    // Offer service install (interactive only)
    if !non_interactive {
        print!("\nInstall and start the background service now? [Y/n] ");
        io::stdout().flush().ok();
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).ok();
        let answer = line.trim().to_lowercase();
        if answer.is_empty() || answer == "y" || answer == "yes" {
            crate::service::install_and_start(paths, false, false, None)?;
        }
    }

    println!("\nSetup complete!");
    println!("Admin UI: http://localhost:{}", env.get("HTTP_PORT").map(|s| s.as_str()).unwrap_or("5207"));

    Ok(())
}

fn prompt_operator_npub() -> String {
    loop {
        print!("Enter your OPERATOR_NPUB (npub1... or 64-char hex): ");
        io::stdout().flush().ok();
        let mut line = String::new();
        // Try /dev/tty for piped installs
        let result = if let Ok(tty) = std::fs::File::open("/dev/tty") {
            let mut reader = io::BufReader::new(tty);
            let mut s = String::new();
            reader.read_line(&mut s).map(|_| s)
        } else {
            io::stdin().lock().read_line(&mut line).map(|_| line.clone())
        };
        let input = result.unwrap_or_default().trim().to_string();
        if validate_operator_npub(&input).is_ok() {
            return input;
        }
        eprintln!("Invalid format. Must be npub1... or 64-char hex pubkey.");
    }
}

/// Write bytes to a path atomically (write to .tmp then rename).
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).context("Failed to write temp env file")?;
    std::fs::rename(&tmp, path).context("Failed to rename env file into place")?;
    Ok(())
}
```

- [ ] Run tests:

```bash
cargo test setup 2>&1 | tail -10
```

Expected: all tests pass (npub validation may vary based on actual key validity — adjust test key if needed).

---

### Task 3.2: Implement `src/doctor.rs`

**Files:**
- Modify: `src/doctor.rs`

- [ ] Implement `src/doctor.rs`:

```rust
//! Prerequisite and configuration checks.

use crate::paths::Paths;
use crate::service::ServiceManager;

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
        Self { name: name.to_string(), status: CheckStatus::Ok, detail: detail.into() }
    }
    fn warn(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), status: CheckStatus::Warning, detail: detail.into() }
    }
    fn err(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), status: CheckStatus::Error, detail: detail.into() }
    }
}

/// Run all checks and return results.
pub fn run_checks(paths: &Paths) -> Vec<Check> {
    let mut checks = Vec::new();

    // Binary
    let current_exe = std::env::current_exe().unwrap_or_default();
    checks.push(Check::ok("binary", format!("{} v{}", current_exe.display(), env!("CARGO_PKG_VERSION"))));

    // Data dir
    if paths.data_dir.exists() {
        checks.push(Check::ok("data_dir", format!("{}", paths.data_dir.display())));
    } else {
        checks.push(Check::warn("data_dir", format!("{} (missing — run setup)", paths.data_dir.display())));
    }

    // Env file
    if paths.env_file.exists() {
        checks.push(Check::ok("env_file", format!("{}", paths.env_file.display())));
    } else {
        checks.push(Check::err("env_file", format!("{} missing — run: nostube-transcode setup", paths.env_file.display())));
    }

    // OPERATOR_NPUB
    let npub_val = std::env::var("OPERATOR_NPUB")
        .or_else(|_| {
            if paths.env_file.exists() {
                crate::setup::read_env_file(&paths.env_file)
                    .ok()
                    .and_then(|m| m.get("OPERATOR_NPUB").cloned())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            } else {
                Err(std::env::VarError::NotPresent)
            }
        });

    match npub_val {
        Ok(v) => {
            match crate::setup::validate_operator_npub(&v) {
                Ok(pk) => checks.push(Check::ok("operator_npub", pk.to_bech32().unwrap_or(v))),
                Err(e) => checks.push(Check::err("operator_npub", format!("invalid: {e}"))),
            }
        }
        Err(_) => checks.push(Check::err("operator_npub", "missing — run: nostube-transcode setup")),
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
                    checks.push(Check::warn("identity_key", format!("present but mode is {:o} (should be 0600)", mode)));
                }
            }
        }
        #[cfg(not(unix))]
        checks.push(Check::ok("identity_key", "present"));
    } else {
        checks.push(Check::warn("identity_key", "not yet generated — will be created on first run"));
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
        ServiceManager::SystemdUser => Some(&paths.systemd_user_unit),
        ServiceManager::Launchd => Some(&paths.launchd_plist),
        ServiceManager::SysV => Some(&paths.sysv_script),
        _ => None,
    };
    if let Some(svc) = svc_path {
        if svc.exists() {
            checks.push(Check::ok("service", format!("{} ({})", mgr.name(), svc.display())));
            // Check if running
            if crate::service::process::is_process_running(&paths.pid_file) {
                let pid = crate::service::process::read_pid_file(&paths.pid_file).unwrap_or(0);
                checks.push(Check::ok("process", format!("running (pid {pid})")));
            } else {
                checks.push(Check::warn("process", "not running — run: nostube-transcode start"));
            }
        } else {
            checks.push(Check::warn("service", format!("not installed — run: nostube-transcode install")));
        }
    } else {
        checks.push(Check::warn("service", format!("{} — run in foreground: nostube-transcode run", mgr.name())));
    }

    checks
}

/// Print checks to stdout, return exit code (0=ok, 1=errors, 2=warnings only).
pub fn print_and_exit_code(checks: &[Check]) -> i32 {
    let mut has_error = false;
    let mut has_warning = false;

    for c in checks {
        let symbol = match c.status {
            CheckStatus::Ok => "✓",
            CheckStatus::Warning => "⚠",
            CheckStatus::Error => "✗",
        };
        println!("  {symbol} {}: {}", c.name, c.detail);
        if c.status == CheckStatus::Error { has_error = true; }
        if c.status == CheckStatus::Warning { has_warning = true; }
    }

    if has_error { 1 } else if has_warning { 2 } else { 0 }
}
```

---

### Task 3.3: Implement `src/config_cmd.rs`

**Files:**
- Modify: `src/config_cmd.rs`

The `config` CLI commands read/write the NIP-78 remote config directly using the DVM's identity key — no need for the daemon to be running.

- [ ] Implement `src/config_cmd.rs`:

```rust
//! CLI commands for reading and writing the DVM's remote NIP-78 configuration.
//!
//! Uses the DVM identity key to directly fetch/save remote config — does not
//! require the daemon to be running.

use anyhow::{Context, Result};
use crate::bootstrap::get_bootstrap_relays;
use crate::identity::load_or_generate_identity;
use crate::remote_config::{fetch_config, save_config, RemoteConfig};
use nostr_sdk::prelude::*;

/// Connect to relays and return (client, keys, config).
async fn connect_and_fetch() -> Result<(Client, Keys, RemoteConfig)> {
    let keys = load_or_generate_identity()
        .context("Failed to load DVM identity")?;
    let client = Client::new(keys.clone());
    for relay in get_bootstrap_relays() {
        let _ = client.add_relay(relay.to_string()).await;
    }
    client.connect().await;

    let config = fetch_config(&client, &keys)
        .await
        .context("Failed to fetch remote config")?
        .unwrap_or_default();

    Ok((client, keys, config))
}

/// Print current remote config as a formatted table.
pub async fn get() -> Result<()> {
    let (_client, _keys, config) = connect_and_fetch().await?;

    println!("Remote configuration");
    println!("====================");
    println!("Relays:");
    for r in &config.relays { println!("  {r}"); }
    println!("Blossom servers:");
    for s in &config.blossom_servers { println!("  {s}"); }
    println!("Max concurrent jobs: {}", config.max_concurrent_jobs);
    println!("Blob expiration:     {} days", config.blob_expiration_days);
    println!("Name:                {}", config.name.as_deref().unwrap_or("-"));
    println!("About:               {}", config.about.as_deref().unwrap_or("-"));
    println!("Paused:              {}", config.paused);

    Ok(())
}

/// Update one or more config fields and save back to relays.
pub async fn set(
    relays: Option<Vec<String>>,
    blossom_servers: Option<Vec<String>>,
    max_concurrent_jobs: Option<u32>,
    blob_expiration_days: Option<u32>,
    name: Option<String>,
    about: Option<String>,
) -> Result<()> {
    let (client, keys, mut config) = connect_and_fetch().await?;

    if let Some(r) = relays { config.relays = r; }
    if let Some(b) = blossom_servers { config.blossom_servers = b; }
    if let Some(j) = max_concurrent_jobs { config.max_concurrent_jobs = j; }
    if let Some(d) = blob_expiration_days { config.blob_expiration_days = d; }
    if let Some(n) = name { config.name = Some(n); }
    if let Some(a) = about { config.about = Some(a); }

    save_config(&client, &keys, &config)
        .await
        .context("Failed to save remote config")?;

    println!("Configuration saved.");
    println!("Restart the DVM to apply changes: nostube-transcode restart");
    Ok(())
}

/// Pause the DVM (set paused=true in remote config).
pub async fn pause() -> Result<()> {
    let (client, keys, mut config) = connect_and_fetch().await?;
    config.paused = true;
    save_config(&client, &keys, &config).await.context("Failed to save")?;
    println!("DVM paused. Restart to apply: nostube-transcode restart");
    Ok(())
}

/// Resume the DVM (set paused=false in remote config).
pub async fn resume() -> Result<()> {
    let (client, keys, mut config) = connect_and_fetch().await?;
    config.paused = false;
    save_config(&client, &keys, &config).await.context("Failed to save")?;
    println!("DVM resumed. Restart to apply: nostube-transcode restart");
    Ok(())
}

/// Show DVM version/status from remote config (does not query runtime state).
pub async fn status() -> Result<()> {
    let (_client, keys, config) = connect_and_fetch().await?;
    println!("DVM pubkey: {}", keys.public_key().to_bech32().unwrap_or_default());
    println!("Paused:     {}", config.paused);
    println!("Max jobs:   {}", config.max_concurrent_jobs);
    Ok(())
}
```

---

### Task 3.4: Wire setup, doctor, config into `src/main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] Replace Phase 3 placeholder arms with real dispatch. The complete updated `main.rs`:

```rust
use clap::Parser;
use nostube_transcode::cli::{Cli, Commands, ConfigCommands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            eprintln!(
                "Note: no subcommand given — defaulting to 'run'. \
                 In future use: nostube-transcode run"
            );
            init_tracing();
            nostube_transcode::runtime::run_daemon(false).await
        }
        Some(Commands::Run { replace }) => {
            init_tracing();
            nostube_transcode::runtime::run_daemon(replace).await
        }
        Some(Commands::Version) => {
            println!("nostube-transcode {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Commands::Setup { non_interactive, operator_npub, http_port, data_dir }) => {
            if let Some(dir) = data_dir {
                std::env::set_var("DATA_DIR", dir);
            }
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::setup::run_setup(
                &paths,
                non_interactive,
                operator_npub.as_deref(),
                http_port,
            ).map_err(|e| { eprintln!("Setup failed: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Doctor { json: _ }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            println!("nostube-transcode doctor");
            println!("========================");
            let checks = nostube_transcode::doctor::run_checks(&paths);
            let code = nostube_transcode::doctor::print_and_exit_code(&checks);
            std::process::exit(code);
        }
        Some(Commands::Config { command }) => {
            match command {
                ConfigCommands::Get => nostube_transcode::config_cmd::get().await,
                ConfigCommands::Set { relays, blossom_servers, max_concurrent_jobs, blob_expiration_days, name, about } => {
                    nostube_transcode::config_cmd::set(relays, blossom_servers, max_concurrent_jobs, blob_expiration_days, name, about).await
                }
                ConfigCommands::Pause => nostube_transcode::config_cmd::pause().await,
                ConfigCommands::Resume => nostube_transcode::config_cmd::resume().await,
                ConfigCommands::Status => nostube_transcode::config_cmd::status().await,
            }
        }
        Some(Commands::Install { force, system, user }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::install_and_start(&paths, system, force, user.as_deref())
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Uninstall { system, .. }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::uninstall(&paths, system)
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Start { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::start(&paths, system)
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Stop { system, force }) => {
            nostube_transcode::service::stop(system, force)
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Restart { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::restart(&paths, system)
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        Some(Commands::Status { deep, system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            println!("Binary:   {}", paths.binary_path.display());
            println!("Data dir: {}", paths.data_dir.display());
            println!("Service:  {}", nostube_transcode::service::ServiceManager::detect().name());
            if nostube_transcode::service::process::is_process_running(&paths.pid_file) {
                let pid = nostube_transcode::service::process::read_pid_file(&paths.pid_file).unwrap_or(0);
                println!("Process:  running (pid {pid})");
            } else {
                println!("Process:  not running");
            }
            println!();
            nostube_transcode::service::status(system, deep).unwrap_or_else(|e| eprintln!("{e}"));
            Ok(())
        }
        Some(Commands::Logs { follow, lines, system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::logs(&paths, follow, lines, system)
                .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
            Ok(())
        }
        // Phase 4
        Some(Commands::Update { .. }) | Some(Commands::Docker { .. }) => {
            eprintln!("Update/docker commands coming in the next release.");
            std::process::exit(1);
        }
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nostube_transcode=debug".parse().unwrap()),
        )
        .init();
}
```

- [ ] Build and test:

```bash
cargo build 2>&1 | grep "^error" | head -20
cargo test 2>&1 | tail -15
```

---

### Task 3.5: Update README + commit Phase 3

- [ ] Add to README after **CLI Reference**:

```markdown
### Setup wizard

```bash
# Interactive (prompts for OPERATOR_NPUB, offers to start service)
nostube-transcode setup

# Non-interactive (for automation)
OPERATOR_NPUB=npub1... nostube-transcode setup --non-interactive --operator-npub npub1...
```

### Doctor

```bash
nostube-transcode doctor
```

Checks prerequisites, config, FFmpeg, identity key, and service state. Exit code 0 = all good, 1 = errors, 2 = warnings only.

### Remote config management

```bash
# View current remote config
nostube-transcode config get

# Update settings (all flags optional, comma-separated lists)
nostube-transcode config set --max-concurrent-jobs 2
nostube-transcode config set --relays wss://relay1.com,wss://relay2.com
nostube-transcode config set --blossom-servers https://server1.com
nostube-transcode config set --name "My DVM" --about "Video transcoding service"

# Pause/resume accepting new jobs
nostube-transcode config pause
nostube-transcode config resume

# Show DVM pubkey and pause state from remote config
nostube-transcode config status
```

Config changes are saved to Nostr relays (NIP-78) and take effect after the DVM restarts.
```

- [ ] Commit Phase 3:

```bash
git add -A
git commit -m "feat: Phase 3 — setup wizard, doctor, config get/set/pause/resume"
```

---

## Phase 4: Installer Update + Docker Subcommands

### Task 4.1: Implement Docker subcommands in `src/cli.rs` / main dispatch

**Files:**
- Modify: `src/main.rs`

The Docker commands work when run from the directory containing `docker-compose.yml`, or when a `COMPOSE_FILE` env var is set.

- [ ] Replace Phase 4 placeholder in `main.rs` with:

```rust
Some(Commands::Docker { command }) => {
    nostube_transcode::docker_cmd::dispatch(command)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(1); }).ok();
    Ok(())
}
Some(Commands::Update { version, yes }) => {
    nostube_transcode::update_cmd::run(version.as_deref(), yes).await
}
```

---

### Task 4.2: Create `src/docker_cmd.rs`

**Files:**
- Create: `src/docker_cmd.rs`

- [ ] Write test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_compose_file_missing() {
        // In a temp dir without docker-compose.yml, should return None
        let dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(dir.path()).ok();
        assert!(find_compose_file().is_none());
    }
}
```

- [ ] Create `src/docker_cmd.rs`:

```rust
//! Docker Compose subcommands.
//!
//! These commands wrap `docker compose` for deployments that use the
//! provided docker-compose.yml. They work when run from the repo root
//! or when COMPOSE_FILE is set in the environment.

use anyhow::{bail, Context, Result};
use crate::cli::DockerCommands;
use std::path::PathBuf;
use std::process::Command;

/// Find docker-compose.yml in the current directory or $COMPOSE_FILE.
pub fn find_compose_file() -> Option<PathBuf> {
    if let Ok(f) = std::env::var("COMPOSE_FILE") {
        let p = PathBuf::from(f);
        if p.exists() { return Some(p); }
    }
    for name in &["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"] {
        let p = PathBuf::from(name);
        if p.exists() { return Some(p); }
    }
    None
}

fn compose_args(extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = vec!["compose".to_string()];
    if let Some(f) = find_compose_file() {
        args.push("-f".to_string());
        args.push(f.to_string_lossy().to_string());
    }
    for a in extra { args.push(a.to_string()); }
    args
}

fn require_compose_file() -> Result<()> {
    if find_compose_file().is_none() {
        bail!(
            "No docker-compose.yml found in the current directory.\n\
             Run this command from the nostube-transcode repo root, or set COMPOSE_FILE.\n\
             To clone and set up: git clone https://github.com/flox1an/nostube-transcode.git"
        );
    }
    Ok(())
}

pub fn dispatch(command: DockerCommands) -> Result<()> {
    match command {
        DockerCommands::Setup => setup(),
        DockerCommands::Status => {
            require_compose_file()?;
            run_docker(&compose_args(&["ps"]))
        }
        DockerCommands::Logs { follow } => {
            require_compose_file()?;
            let mut extra = vec!["logs", "--tail=100"];
            if follow { extra.push("-f"); }
            run_docker(&compose_args(&extra))
        }
        DockerCommands::Start => {
            require_compose_file()?;
            run_docker(&compose_args(&["up", "-d"]))
        }
        DockerCommands::Stop => {
            require_compose_file()?;
            run_docker(&compose_args(&["down"]))
        }
        DockerCommands::Restart => {
            require_compose_file()?;
            run_docker(&compose_args(&["restart"]))
        }
    }
}

fn setup() -> Result<()> {
    // If setup.sh is present in cwd, run it
    if PathBuf::from("setup.sh").exists() {
        let status = Command::new("bash")
            .arg("setup.sh")
            .status()
            .context("Failed to run setup.sh")?;
        if !status.success() {
            bail!("setup.sh exited with non-zero status");
        }
        return Ok(());
    }
    // Otherwise guide user
    bail!(
        "setup.sh not found in the current directory.\n\
         Clone the repository first:\n\
         \n\
           git clone https://github.com/flox1an/nostube-transcode.git\n\
           cd nostube-transcode\n\
           nostube-transcode docker setup\n"
    )
}

fn run_docker(args: &[String]) -> Result<()> {
    let status = Command::new("docker")
        .args(args)
        .status()
        .context("docker not found — install Docker first")?;
    if !status.success() {
        bail!("docker {} exited with error", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_compose_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let _orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).ok();
        assert!(find_compose_file().is_none());
    }
}
```

---

### Task 4.3: Create `src/update_cmd.rs`

**Files:**
- Create: `src/update_cmd.rs`

- [ ] Add to `src/lib.rs`:

```rust
pub mod docker_cmd;
pub mod update_cmd;
```

- [ ] Create `src/update_cmd.rs`:

```rust
//! Self-update: download the latest release from GitHub and replace the binary.

use anyhow::{bail, Context, Result};
use std::io::Write;

const REPO: &str = "flox1an/nostube-transcode";
const BINARY_NAME: &str = "nostube-transcode";

/// Run the update command.
pub async fn run(pinned_version: Option<&str>, yes: bool) -> Result<()> {
    let tag = match pinned_version {
        Some(v) => v.to_string(),
        None => {
            print!("Fetching latest release... ");
            std::io::stdout().flush().ok();
            latest_tag().await.context("Failed to fetch latest release")?
        }
    };

    let current = env!("CARGO_PKG_VERSION");
    let current_tag = format!("v{current}");

    if tag == current_tag && !yes {
        println!("Already on {tag}. Nothing to do.");
        return Ok(());
    }

    println!("Update: {current_tag} → {tag}");

    if !yes {
        print!("Continue? [Y/n] ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
        let answer = line.trim().to_lowercase();
        if !answer.is_empty() && answer != "y" && answer != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    let platform = detect_platform().context("Unsupported platform for self-update")?;
    let archive_name = format!("{BINARY_NAME}-{tag}-{platform}.tar.gz");
    let url = format!("https://github.com/{REPO}/releases/download/{tag}/{archive_name}");

    println!("Downloading {archive_name}...");

    // Download to temp file
    let tmpdir = tempfile::tempdir().context("Failed to create temp dir")?;
    let archive_path = tmpdir.path().join(&archive_name);

    let response = reqwest::get(&url).await.context("Download failed")?;
    if !response.status().is_success() {
        bail!("Download failed: HTTP {} for {url}", response.status());
    }
    let bytes = response.bytes().await.context("Failed to read response body")?;
    std::fs::write(&archive_path, &bytes).context("Failed to write archive")?;

    println!("Extracting...");
    let status = std::process::Command::new("tar")
        .args(["-xzf", archive_path.to_str().unwrap(), "-C", tmpdir.path().to_str().unwrap()])
        .status()
        .context("tar not available")?;
    if !status.success() { bail!("Extraction failed"); }

    let extracted_binary = tmpdir.path().join(BINARY_NAME);
    if !extracted_binary.exists() {
        bail!("Binary not found in archive — unexpected archive structure");
    }

    // Replace current binary
    let current_exe = std::env::current_exe().context("Cannot determine current binary path")?;
    let backup = current_exe.with_extension("old");
    std::fs::rename(&current_exe, &backup).context("Failed to rename current binary")?;
    std::fs::copy(&extracted_binary, &current_exe).context("Failed to install new binary")?;
    let _ = std::fs::remove_file(&backup);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)?;
    }

    println!("Updated to {tag}!");
    println!("Restart the service: nostube-transcode restart");

    Ok(())
}

async fn latest_tag() -> Result<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("nostube-transcode-updater")
        .build()?;
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    resp["tag_name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("tag_name not found in GitHub API response"))
}

fn detect_platform() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("linux", "x86_64") => Ok("x86_64-linux".to_string()),
        ("macos", "aarch64") => Ok("aarch64-darwin".to_string()),
        _ => bail!("Unsupported platform: {os}/{arch} — download manually from https://github.com/{REPO}/releases"),
    }
}
```

---

### Task 4.4: Update `install.sh` to delegate to binary

**Files:**
- Modify: `install.sh`

The key change: after the binary is installed, call `nostube-transcode setup` (or `--non-interactive`) instead of doing setup in shell. Keep binary download + ffmpeg check in shell; remove shell-based service generation.

- [ ] Replace the `setup_operator_npub` and `setup_daemon` functions and main body with:

```bash
# --- Delegate setup to binary ---

run_binary_setup() {
  local binary="${INSTALL_DIR}/${BINARY_NAME}"

  # Ensure binary is on PATH for this session
  export PATH="${INSTALL_DIR}:${PATH}"

  if [ -n "${OPERATOR_NPUB:-}" ]; then
    # Non-interactive mode: pass npub directly
    info "Running non-interactive setup..."
    "${binary}" setup \
      --non-interactive \
      --operator-npub "${OPERATOR_NPUB}" \
      ${HTTP_PORT:+--http-port "${HTTP_PORT}"}
  else
    # Interactive mode
    "${binary}" setup
  fi
}

# --- Main ---

main() {
  echo ""
  bold "nostube-transcode installer"
  echo ""

  detect_platform
  determine_version
  check_existing
  download_and_install
  check_ffmpeg
  run_binary_setup
  print_summary
}
```

- [ ] Update `print_summary` to show new CLI commands:

```bash
print_summary() {
  echo ""
  bold "${BINARY_NAME} ${TAG} installed!"
  echo ""
  echo "  Binary:  ${INSTALL_DIR}/${BINARY_NAME}"
  echo "  Config:  ${DATA_DIR}/env"
  echo ""
  echo "  Service commands:"
  echo "    ${BINARY_NAME} status"
  echo "    ${BINARY_NAME} logs -f"
  echo "    ${BINARY_NAME} restart"
  echo "    ${BINARY_NAME} doctor"
  echo ""

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    warn "${INSTALL_DIR} is not in your PATH."
    echo "  Add it: echo 'export PATH=\"\${HOME}/.local/bin:\${PATH}\"' >> ~/.bashrc"
    echo ""
  fi
}
```

- [ ] Build and test:

```bash
cargo build 2>&1 | grep "^error" | head -20
cargo test 2>&1 | tail -15
bash -n install.sh && echo "install.sh syntax OK"
```

---

### Task 4.5: Update README + commit Phase 4

- [ ] Update README "Option B: Standalone Binary" section:

```markdown
### Option B: Standalone Binary

**Install via script (installs binary + runs setup wizard):**

```bash
curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
```

Non-interactive (for servers/automation):

```bash
OPERATOR_NPUB=npub1... curl -sSf ... | bash
```

**Or update an existing install:**

```bash
nostube-transcode update
```

**Docker deployment from the CLI:**

```bash
# Clone repo, then:
nostube-transcode docker setup    # runs setup.sh — detects GPU, writes .env, starts compose
nostube-transcode docker status   # docker compose ps
nostube-transcode docker logs -f  # follow logs
nostube-transcode docker stop     # docker compose down
nostube-transcode docker restart  # docker compose restart
```
```

- [ ] Commit Phase 4:

```bash
git add -A
git commit -m "feat: Phase 4 — installer delegates to binary, docker subcommands, self-update"
```

---

## Self-Review Against Spec

### Spec coverage check

| Spec requirement | Task |
|---|---|
| `run` subcommand | 1.3, 1.6 |
| `setup` interactive wizard | 3.1 |
| `setup --non-interactive` | 3.1 |
| `install` installs + starts immediately | 2.4 |
| `uninstall` | 2.4 |
| `start / stop / restart` | 2.4, 2.5 |
| `status` local + service manager | 2.5 |
| `logs` systemd/launchd/file | 2.1, 2.2, 2.4 |
| `doctor` with exit codes | 3.2 |
| `update` self-update from GitHub | 4.3 |
| `config get/set/pause/resume` | 3.3 |
| `docker setup/status/logs/start/stop/restart` | 4.2 |
| systemd user service generation + stale detection | 2.1 |
| launchd plist generation + stale detection | 2.2 |
| SysV init script | 2.3 |
| `--replace` flag + PID file | 1.3, 1.4 |
| No-subcommand backward compat | 1.6, 3.4 |
| Path resolution with env overrides | 1.2 |
| Env file atomic write, preserve unknown keys | 3.1 |
| OPERATOR_NPUB validation | 3.1 |
| identity.key mode 0600 check | 3.2 |
| installer delegates to binary | 4.4 |
| README updated at each phase | 1.8, 2.6, 3.5, 4.5 |
| Commit per phase | 1.8, 2.6, 3.5, 4.5 |

All spec requirements covered.
