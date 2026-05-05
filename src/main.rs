use clap::Parser;
use nostube_transcode::cli::{Cli, Commands, ConfigCommands, DockerCommands};

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

        // ── Setup ──────────────────────────────────────────────────────────
        Some(Commands::Setup {
            non_interactive,
            operator_npub,
            http_port,
            data_dir,
        }) => {
            if let Some(dir) = data_dir {
                unsafe { std::env::set_var("DATA_DIR", dir) };
            }
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::setup::run_setup(
                &paths,
                non_interactive,
                operator_npub.as_deref(),
                http_port,
            )
            .unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                std::process::exit(1);
            });
            Ok(())
        }

        // ── Doctor ─────────────────────────────────────────────────────────
        Some(Commands::Doctor { json }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            let checks = nostube_transcode::doctor::run_checks(&paths);
            if json {
                // Minimal JSON output
                let entries: Vec<serde_json::Value> = checks
                    .iter()
                    .map(|c| {
                        let status = match c.status {
                            nostube_transcode::doctor::CheckStatus::Ok => "ok",
                            nostube_transcode::doctor::CheckStatus::Warning => "warning",
                            nostube_transcode::doctor::CheckStatus::Error => "error",
                        };
                        serde_json::json!({
                            "name": c.name,
                            "status": status,
                            "detail": c.detail,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&entries)?);
                Ok(())
            } else {
                let code = nostube_transcode::doctor::print_and_exit_code(&checks);
                std::process::exit(code);
            }
        }

        // ── Config ─────────────────────────────────────────────────────────
        Some(Commands::Config { command }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            match command {
                ConfigCommands::Get => {
                    nostube_transcode::config_cmd::get(&paths).await?;
                }
                ConfigCommands::Set {
                    relays,
                    blossom_servers,
                    max_concurrent_jobs,
                    blob_expiration_days,
                    name,
                    about,
                } => {
                    nostube_transcode::config_cmd::set(
                        &paths,
                        relays,
                        blossom_servers,
                        max_concurrent_jobs,
                        blob_expiration_days,
                        name,
                        about,
                    )
                    .await?;
                }
                ConfigCommands::Pause => {
                    nostube_transcode::config_cmd::pause(&paths).await?;
                }
                ConfigCommands::Resume => {
                    nostube_transcode::config_cmd::resume(&paths).await?;
                }
                ConfigCommands::Status => {
                    nostube_transcode::config_cmd::status(&paths).await?;
                }
            }
            Ok(())
        }

        // ── Service management ─────────────────────────────────────────────
        Some(Commands::Install { force, system, user }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::install_and_start(
                &paths,
                system,
                force,
                user.as_deref(),
            )
            .unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                std::process::exit(1);
            });
            Ok(())
        }
        Some(Commands::Uninstall { system, .. }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::uninstall(&paths, system)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                });
            Ok(())
        }
        Some(Commands::Start { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::start(&paths, system)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                });
            Ok(())
        }
        Some(Commands::Stop { system, force }) => {
            nostube_transcode::service::stop(system, force)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                });
            Ok(())
        }
        Some(Commands::Restart { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::restart(&paths, system)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                });
            Ok(())
        }
        Some(Commands::Status { deep, system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            println!("Binary:   {}", paths.binary_path.display());
            println!("Data dir: {}", paths.data_dir.display());
            println!(
                "Service:  {}",
                nostube_transcode::service::ServiceManager::detect().name()
            );
            if nostube_transcode::service::process::is_process_running(&paths.pid_file) {
                let pid = nostube_transcode::service::process::read_pid_file(&paths.pid_file)
                    .unwrap_or(0);
                println!("Process:  running (pid {pid})");
            } else {
                println!("Process:  not running");
            }
            println!();
            nostube_transcode::service::status(system, deep)
                .unwrap_or_else(|e| eprintln!("{e}"));
            Ok(())
        }
        Some(Commands::Logs { follow, lines, system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::logs(&paths, follow, lines, system)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                });
            Ok(())
        }

        // ── Update ─────────────────────────────────────────────────────────
        Some(Commands::Update { yes, version }) => {
            if version.is_some() {
                eprintln!("--version pinning not yet implemented. Updating to latest.");
            }
            nostube_transcode::update_cmd::run(yes, false).await?;
            Ok(())
        }

        // ── Docker ─────────────────────────────────────────────────────────
        Some(Commands::Docker { command }) => {
            match command {
                DockerCommands::Setup => {
                    nostube_transcode::docker_cmd::setup()?;
                }
                DockerCommands::Status => {
                    nostube_transcode::docker_cmd::docker_status()?;
                }
                DockerCommands::Logs { follow } => {
                    nostube_transcode::docker_cmd::logs(follow)?;
                }
                DockerCommands::Start => {
                    nostube_transcode::docker_cmd::start()?;
                }
                DockerCommands::Stop => {
                    nostube_transcode::docker_cmd::stop()?;
                }
                DockerCommands::Restart => {
                    nostube_transcode::docker_cmd::restart()?;
                }
            }
            Ok(())
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
