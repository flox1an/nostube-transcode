use tokio::signal;
use tracing::info;

use dvm_video_processing::admin::run_admin_listener;
use dvm_video_processing::blossom::BlossomClient;
use dvm_video_processing::dvm::{AnnouncementPublisher, JobHandler};
use dvm_video_processing::nostr::{EventPublisher, SubscriptionManager};
use dvm_video_processing::startup::initialize;
use dvm_video_processing::video::{HwAccel, VideoProcessor};
use dvm_video_processing::web::run_server;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dvm_video_processing=debug".parse()?),
        )
        .init();

    info!("Starting DVM Video Processing Service");

    // Remote config mode - zero config startup
    info!("Starting in remote config mode...");

    let startup = initialize().await.expect("Failed to initialize DVM");

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
        let pairing = startup.pairing.clone();
        let config = startup.config.clone();
        async move {
            run_admin_listener(client, keys, state, pairing, config).await;
        }
    });

    if startup.needs_pairing {
        // In pairing mode, wait for admin to claim
        info!("Waiting for admin pairing...");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let state = startup.state.read().await;
            if state.config.has_admin() {
                info!("Admin paired, starting normal operation");
                
                // Add configured relays after pairing
                if !state.config.relays.is_empty() {
                    info!("Adding configured relays...");
                    for relay in &state.config.relays {
                        if let Err(e) = startup.client.add_relay(relay.clone()).await {
                            tracing::warn!("Failed to add relay {}: {}", relay, e);
                        }
                    }
                }
                break;
            }
        }
    }

    // Admin is now configured - start announcement publisher
    info!(
        "Starting DVM announcement publisher (admin: {})",
        startup.config.admin_pubkey.as_deref().unwrap_or("MISSING")
    );
    let hwaccel = HwAccel::detect();
    let publisher = Arc::new(EventPublisher::new(
        startup.config.clone(),
        startup.client.clone(),
    ));
    let announcement_publisher = AnnouncementPublisher::new(
        startup.config.clone(),
        publisher,
        hwaccel,
    );

    let announcement_handle = tokio::spawn(async move {
        announcement_publisher.run().await;
    });

    // Create job processing channel
    let (job_tx, job_rx) = tokio::sync::mpsc::channel(32);

    // Spawn subscription manager (listens for kind 5207 requests)
    let subscription_handle = tokio::spawn({
        let config = startup.config.clone();
        async move {
            match SubscriptionManager::new(config).await {
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
    ));
    let blossom = Arc::new(BlossomClient::new(startup.config.clone()));
    let processor = Arc::new(VideoProcessor::new(startup.config.clone()));
    let job_handler = JobHandler::new(
        startup.config.clone(),
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
