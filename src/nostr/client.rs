use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::dvm::events::{JobContext, DVM_VIDEO_TRANSFORM_REQUEST_KIND};
use crate::error::DvmError;

pub struct SubscriptionManager {
    #[allow(dead_code)]
    config: Arc<Config>,
    client: Client,
}

impl SubscriptionManager {
    pub async fn new(config: Arc<Config>) -> Result<Self, DvmError> {
        let client = Client::new(&config.nostr_keys);

        // Add relays
        for relay in &config.nostr_relays {
            client.add_relay(relay.as_str()).await?;
        }

        Ok(Self { config, client })
    }

    /// Connect to relays and start listening for DVM requests
    pub async fn run(&self, job_tx: mpsc::Sender<JobContext>) -> Result<(), DvmError> {
        info!("Connecting to relays...");
        self.client.connect().await;

        // Subscribe to DVM requests
        let filter = Filter::new()
            .kind(DVM_VIDEO_TRANSFORM_REQUEST_KIND)
            .since(Timestamp::now());

        self.client.subscribe(vec![filter], None).await?;

        info!("Subscribed to DVM video transform requests");

        // Deduplication set wrapped in Arc<Mutex> for sharing across async closure
        let seen: Arc<Mutex<HashSet<EventId>>> = Arc::new(Mutex::new(HashSet::new()));

        // Handle events
        self.client
            .handle_notifications(|notification| {
                let job_tx = job_tx.clone();
                let seen = seen.clone();

                async move {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind == DVM_VIDEO_TRANSFORM_REQUEST_KIND {
                            let mut seen_guard = seen.lock().await;
                            if !seen_guard.contains(&event.id) {
                                seen_guard.insert(event.id);
                                drop(seen_guard); // Release lock before async operations

                                debug!(event_id = %event.id, "Received DVM request");

                                match JobContext::from_event((*event).clone()) {
                                    Ok(context) => {
                                        if let Err(e) = job_tx.send(context).await {
                                            error!("Failed to queue job: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Rejected job: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Ok(false) // Continue handling
                }
            })
            .await?;

        Ok(())
    }

    /// Get the underlying client for publishing
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Disconnect from all relays
    pub async fn disconnect(&self) {
        let _ = self.client.disconnect().await;
    }
}
