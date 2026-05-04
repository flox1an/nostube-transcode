use clap::Parser;
use nostube_transcode::cli::{Cli, Commands};

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
        Some(Commands::Install { force, system, user }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::install_and_start(
                &paths,
                system,
                force,
                user.as_deref(),
            )
            .map_err(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            })
            .ok();
            Ok(())
        }
        Some(Commands::Uninstall { system, .. }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::uninstall(&paths, system)
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                })
                .ok();
            Ok(())
        }
        Some(Commands::Start { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::start(&paths, system)
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                })
                .ok();
            Ok(())
        }
        Some(Commands::Stop { system, force }) => {
            nostube_transcode::service::stop(system, force)
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                })
                .ok();
            Ok(())
        }
        Some(Commands::Restart { system }) => {
            let paths = nostube_transcode::paths::Paths::resolve();
            nostube_transcode::service::restart(&paths, system)
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                })
                .ok();
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
                .map_err(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                })
                .ok();
            Ok(())
        }
        Some(Commands::Setup { .. })
        | Some(Commands::Doctor { .. })
        | Some(Commands::Config { .. }) => {
            eprintln!("Setup/doctor/config commands coming in the next release.");
            std::process::exit(1);
        }
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
