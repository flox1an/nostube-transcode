use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::info;

use dvm_video_processing::blossom::{BlobCleanup, BlossomClient};
use dvm_video_processing::config::Config;
use dvm_video_processing::dvm::{AnnouncementPublisher, JobContext, JobHandler};
use dvm_video_processing::nostr::{EventPublisher, SubscriptionManager};
use dvm_video_processing::video::VideoProcessor;
use dvm_video_processing::web;

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

    let config = Arc::new(Config::from_env()?);

    // Create shared components
    let blossom = Arc::new(BlossomClient::new(config.clone()));
    let processor = Arc::new(VideoProcessor::new(config.clone()));

    // Channel for job processing
    let (job_tx, job_rx) = mpsc::channel::<JobContext>(100);

    // Create subscription manager
    let sub_manager = Arc::new(SubscriptionManager::new(config.clone()).await?);

    // Create publisher using the same client
    let publisher = Arc::new(EventPublisher::new(
        config.clone(),
        sub_manager.client().clone(),
    ));

    // Create job handler
    let handler = Arc::new(JobHandler::new(
        config.clone(),
        publisher.clone(),
        blossom.clone(),
        processor.clone(),
    ));

    // Create announcement publisher
    let announcement = Arc::new(AnnouncementPublisher::new(
        config.clone(),
        publisher,
        processor.hwaccel(),
    ));

    // Create cleanup scheduler
    let cleanup = Arc::new(BlobCleanup::new(config.clone(), blossom));

    // Spawn subscription manager
    let sub_handle = tokio::spawn({
        let sub_manager = sub_manager.clone();
        async move { sub_manager.run(job_tx).await }
    });

    // Spawn job processor
    let job_handle = tokio::spawn({
        let handler = handler.clone();
        async move { handler.run(job_rx).await }
    });

    // Spawn cleanup scheduler
    let cleanup_handle = tokio::spawn({
        let cleanup = cleanup.clone();
        async move { cleanup.run().await }
    });

    // Spawn announcement publisher
    let announcement_handle = tokio::spawn({
        let announcement = announcement.clone();
        async move { announcement.run().await }
    });

    // Spawn HTTP server
    let web_handle = tokio::spawn({
        let config = config.clone();
        async move { web::run_server(config).await }
    });

    info!(
        pubkey = %config.nostr_keys.public_key(),
        "DVM is running. Press Ctrl+C to shutdown."
    );

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("Shutting down...");

    // Cancel all tasks
    sub_handle.abort();
    job_handle.abort();
    cleanup_handle.abort();
    announcement_handle.abort();
    web_handle.abort();

    // Disconnect from relays
    sub_manager.disconnect().await;

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
