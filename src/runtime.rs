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
            eprintln!(
                "Warning: Error loading env file from {:?}: {}",
                env_path, e
            );
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
        async move {
            run_admin_listener(client, keys, state, config, config_notify).await;
        }
    });

    let hwaccel = HwAccel::detect();
    let publisher = Arc::new(EventPublisher::new(
        startup.config.clone(),
        startup.client.clone(),
        startup.state.clone(),
    ));
    let announcement_publisher = AnnouncementPublisher::new(
        startup.config.clone(),
        startup.state.clone(),
        publisher,
        hwaccel,
        config_notify,
    );
    let announcement_handle =
        tokio::spawn(async move { announcement_publisher.run().await });

    let (job_tx, job_rx) = tokio::sync::mpsc::channel(32);
    let subscription_handle = tokio::spawn({
        let config = startup.config.clone();
        let client = startup.client.clone();
        let state = startup.state.clone();
        async move {
            match SubscriptionManager::new(config, client, state).await {
                Ok(manager) => {
                    if let Err(e) = manager.run(job_tx).await {
                        tracing::error!("Subscription manager error: {}", e);
                    }
                }
                Err(e) => tracing::error!("Failed to create subscription manager: {}", e),
            }
        }
    });

    let job_publisher = Arc::new(EventPublisher::new(
        startup.config.clone(),
        startup.client.clone(),
        startup.state.clone(),
    ));
    let blossom = Arc::new(BlossomClient::new(
        startup.config.clone(),
        startup.state.clone(),
    ));
    let processor = Arc::new(VideoProcessor::new(startup.config.clone()));
    let job_handler = Arc::new(JobHandler::new(
        startup.config.clone(),
        startup.state.clone(),
        job_publisher,
        blossom,
        processor,
    ));
    let job_handle = tokio::spawn(async move { job_handler.run(job_rx).await });

    info!("Remote config mode active. Press Ctrl+C to shutdown.");
    shutdown_signal().await;

    info!("Shutting down...");
    if let Some(h) = web_handle {
        h.abort();
    }
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
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
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

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
