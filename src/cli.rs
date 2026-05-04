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
    /// Show DVM pubkey and pause state from remote config
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
    /// docker compose up -d
    Start,
    /// docker compose down
    Stop,
    /// docker compose restart
    Restart,
}

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
        let cli =
            Cli::try_parse_from(["nostube-transcode", "install", "--system"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Install { system: true, .. })
        ));
    }

    #[test]
    fn test_cli_parses_logs() {
        let cli = Cli::try_parse_from([
            "nostube-transcode",
            "logs",
            "-n",
            "100",
            "--follow",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Logs {
                follow: true,
                lines: 100,
                ..
            })
        ));
    }

    #[test]
    fn test_cli_parses_config_set() {
        let cli = Cli::try_parse_from([
            "nostube-transcode",
            "config",
            "set",
            "--max-concurrent-jobs",
            "3",
            "--name",
            "My DVM",
        ])
        .unwrap();
        assert!(matches!(cli.command, Some(Commands::Config { .. })));
    }

    #[test]
    fn test_cli_parses_docker_setup() {
        let cli =
            Cli::try_parse_from(["nostube-transcode", "docker", "setup"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Docker {
                command: DockerCommands::Setup
            })
        ));
    }

    #[test]
    fn verify_cli_structure() {
        Cli::command().debug_assert();
    }
}
