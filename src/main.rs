use tokio::signal;
use tracing::info;

use nostube_transcode::admin::run_admin_listener;
use nostube_transcode::blossom::BlossomClient;
use nostube_transcode::dvm::{AnnouncementPublisher, JobHandler};
use nostube_transcode::nostr::{EventPublisher, SubscriptionManager};
use nostube_transcode::startup::initialize;
use nostube_transcode::video::{HwAccel, VideoProcessor};
use nostube_transcode::web::run_server;
use std::sync::Arc;
use tokio::sync::Notify;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env or data/env before doing anything else
    // Try .env in current directory first
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("Warning: Error loading .env file: {}", e);
        }
    }

    // Also try to load from the data directory's env file (used by the installer)
    let env_path = nostube_transcode::identity::default_data_dir().join("env");
    if env_path.exists() {
        if let Err(e) = dotenvy::from_path(&env_path) {
            eprintln!("Warning: Error loading env file from {:?}: {}", env_path, e);
        }
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nostube_transcode=debug".parse()?),
        )
        .init();

    info!("Starting DVM Video Processing Service");

    // Remote config mode - zero config startup
    info!("Starting in remote config mode...");

    let startup = initialize().await.expect("Failed to initialize DVM");

    // Create config change notifier (shared between admin handler and announcement publisher)
    let config_notify = Arc::new(Notify::new());

    // Spawn web server immediately
    let web_handle = tokio::spawn({
        let config = startup.config.clone();
        async move {
            if let Err(e) = run_server(config).await {
                tracing::error!("Web server error: {}", e);
            }
        }
    });

    // Spawn admin listener
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

    // Start announcement publisher
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

    let announcement_handle = tokio::spawn(async move {
        announcement_publisher.run().await;
    });

    // Create job processing channel
    let (job_tx, job_rx) = tokio::sync::mpsc::channel(32);

    // Spawn subscription manager (listens for kind 5207 requests)
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
                Err(e) => {
                    tracing::error!("Failed to create subscription manager: {}", e);
                }
            }
        }
    });

    // Spawn job handler
    let job_publisher = Arc::new(EventPublisher::new(
        startup.config.clone(),
        startup.client.clone(),
        startup.state.clone(),
    ));
    let blossom = Arc::new(BlossomClient::new(startup.config.clone(), startup.state.clone()));
    let processor = Arc::new(VideoProcessor::new(startup.config.clone()));
    let job_handler = JobHandler::new(
        startup.config.clone(),
        startup.state.clone(),
        job_publisher,
        blossom,
        processor,
    );

    let job_handle = tokio::spawn(async move {
        job_handler.run(job_rx).await;
    });

    info!("Remote config mode active. Press Ctrl+C to shutdown.");
    shutdown_signal().await;

    info!("Shutting down...");
    web_handle.abort();
    admin_handle.abort();
    announcement_handle.abort();
    subscription_handle.abort();
    job_handle.abort();
    let _ = startup.client.disconnect().await;

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
