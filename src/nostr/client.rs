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
    pub async fn new(config: Arc<Config>, client: Client) -> Result<Self, DvmError> {
        Ok(Self { config, client })
    }

    /// Get the DVM keys for encryption/decryption
    pub fn keys(&self) -> &Keys {
        &self.config.nostr_keys
    }

    /// Connect to relays and start listening for DVM requests
    pub async fn run(&self, job_tx: mpsc::Sender<JobContext>) -> Result<(), DvmError> {
        info!("Connecting to relays...");
        self.client.connect().await;

        // Wait for at least one relay to connect before subscribing
        // This prevents "relay not connected" errors from nostr-sdk
        let mut connected = false;
        for i in 0..20 {
            let relays = self.client.relays().await;
            for relay in relays.values() {
                if relay.is_connected().await {
                    connected = true;
                    break;
                }
            }
            
            if connected {
                break;
            }

            if i % 5 == 0 && i > 0 {
                info!("Waiting for relays to connect... (attempt {})", i);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        if !connected {
            warn!("Starting subscription without any connected relays. This may fail.");
        }

        // Subscribe to DVM requests addressed to this DVM
        let dvm_pubkey = self.config.nostr_keys.public_key();
        let filter = Filter::new()
            .kind(DVM_VIDEO_TRANSFORM_REQUEST_KIND)
            .custom_tag(SingleLetterTag::lowercase(Alphabet::P), [dvm_pubkey.to_hex()])
            .since(Timestamp::now());

        // Try to subscribe with retries
        let mut last_error = None;
        for i in 0..5 {
            match self.client.subscribe(vec![filter.clone()], None).await {
                Ok(_) => {
                    info!("Subscribed to DVM video transform requests");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    warn!("Subscription attempt {} failed: {}. Retrying in 2s...", i + 1, e);
                    last_error = Some(e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }

        if let Some(e) = last_error {
            error!("Failed to subscribe to DVM requests after multiple attempts: {}", e);
            return Err(DvmError::Nostr(e));
        }

        // Deduplication set wrapped in Arc<Mutex> for sharing across async closure
        let seen: Arc<Mutex<HashSet<EventId>>> = Arc::new(Mutex::new(HashSet::new()));
        let keys = self.config.nostr_keys.clone();

        // Handle events
        self.client
            .handle_notifications(|notification| {
                let job_tx = job_tx.clone();
                let seen = seen.clone();
                let keys = keys.clone();

                async move {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind == DVM_VIDEO_TRANSFORM_REQUEST_KIND {
                            let mut seen_guard = seen.lock().await;
                            if !seen_guard.contains(&event.id) {
                                seen_guard.insert(event.id);
                                drop(seen_guard); // Release lock before async operations

                                debug!(event_id = %event.id, "Received DVM request");

                                // Use from_event_with_keys to handle encrypted requests
                                match JobContext::from_event_with_keys((*event).clone(), &keys) {
                                    Ok(context) => {
                                        if context.was_encrypted {
                                            debug!(event_id = %context.event_id(), "Decrypted encrypted request");
                                        }
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
