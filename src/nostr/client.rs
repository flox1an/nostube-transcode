use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::dvm_state::SharedDvmState;
use crate::dvm::events::{JobContext, DVM_VIDEO_TRANSFORM_REQUEST_KIND, DVM_STATUS_KIND};
use crate::error::DvmError;

pub struct SubscriptionManager {
    #[allow(dead_code)]
    config: Arc<Config>,
    client: Client,
    state: SharedDvmState,
}

impl SubscriptionManager {
    pub async fn new(config: Arc<Config>, client: Client, state: SharedDvmState) -> Result<Self, DvmError> {
        Ok(Self { config, client, state })
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

        // Background cleanup task for expired bids
        let state_for_cleanup = self.state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                state_for_cleanup.write().await.cleanup_bids();
            }
        });

        // Subscribe to DVM requests, selection feedback, and gift wraps (Cashu)
        let dvm_pubkey = self.config.nostr_keys.public_key();
        let filter = Filter::new()
            .kinds(vec![
                DVM_VIDEO_TRANSFORM_REQUEST_KIND,
                DVM_STATUS_KIND,
                Kind::GiftWrap,
            ])
            .since(Timestamp::now());
            
        // For status and gift wrap, we only care about those addressed to us
        let directed_filter = Filter::new()
            .kinds(vec![DVM_STATUS_KIND, Kind::GiftWrap])
            .pubkey(dvm_pubkey)
            .since(Timestamp::now());

        // Try to subscribe with retries
        let mut last_error = None;
        for i in 0..5 {
            match self.client.subscribe(vec![filter.clone(), directed_filter.clone()], None).await {
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
                let state = self.state.clone();

                async move {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind == DVM_VIDEO_TRANSFORM_REQUEST_KIND {
                            let mut seen_guard = seen.lock().await;
                            if !seen_guard.contains(&event.id) {
                                seen_guard.insert(event.id);
                                drop(seen_guard);

                                debug!(event_id = %event.id, "Received DVM request");

                                match JobContext::from_event_with_keys((*event).clone(), &keys) {
                                    Ok(context) => {
                                        if let Err(e) = job_tx.send(context).await {
                                            error!("Failed to queue job: {}", e);
                                        }
                                    }
                                    Err(e) => warn!("Rejected job: {}", e),
                                }
                            }
                        } else if event.kind == DVM_STATUS_KIND {
                            // Check if this is a "selection" feedback from a user
                            let is_approved = event.tags.iter().any(|t| {
                                let parts = t.as_slice();
                                parts.len() >= 2 && parts[0] == "status" && parts[1] == "approved"
                            });

                            if is_approved {
                                let job_id = event.tags.iter().find_map(|t| {
                                    let parts = t.as_slice();
                                    if parts.len() >= 2 && parts[0] == "e" {
                                        EventId::parse(&parts[1]).ok()
                                    } else {
                                        None
                                    }
                                });

                                if let Some(id) = job_id {
                                    let mut state_guard = state.write().await;
                                    if let Some(bid) = state_guard.take_bid(&id) {
                                        info!(job_id = %id, "Received 'approved' feedback, starting pending job");
                                        if let Err(e) = job_tx.send(bid.context).await {
                                            error!("Failed to queue approved job: {}", e);
                                        }
                                    }
                                }
                            }
                        } else if event.kind == Kind::GiftWrap {
                            // Handle NIP-17 GiftWrap (potentially for Cashu tokens or private feedback)
                            if let Ok(UnwrappedGift { rumor, .. }) = self.client.unwrap_gift_wrap(&event).await {
                                if rumor.kind == DVM_STATUS_KIND {
                                    // Check if this is an "approved" feedback in a Rumor
                                    let is_approved = rumor.tags.iter().any(|t| {
                                        let parts = t.as_slice();
                                        parts.len() >= 2 && parts[0] == "status" && parts[1] == "approved"
                                    });

                                    if is_approved {
                                        let job_id = rumor.tags.iter().find_map(|t| {
                                            let parts = t.as_slice();
                                            if parts.len() >= 2 && parts[0] == "e" {
                                                EventId::parse(&parts[1]).ok()
                                            } else {
                                                None
                                            }
                                        });

                                        if let Some(id) = job_id {
                                            let mut state_guard = state.write().await;
                                            if let Some(bid) = state_guard.take_bid(&id) {
                                                info!(job_id = %id, "Received private 'approved' feedback via NIP-17, starting job");
                                                if let Err(e) = job_tx.send(bid.context).await {
                                                    error!("Failed to queue approved job: {}", e);
                                                }
                                            }
                                        }
                                    }
                                } else if rumor.kind == DVM_VIDEO_TRANSFORM_REQUEST_KIND {
                                    // Directed request within a GiftWrap (Selection + Payment)
                                    match JobContext::from_rumor_with_keys(rumor, &keys) {
                                        Ok(context) => {
                                            if context.cashu_token.is_some() {
                                                debug!(job_id = %context.event_id(), "Received directed request with Cashu token via NIP-17");
                                            }
                                            if let Err(e) = job_tx.send(context).await {
                                                error!("Failed to queue job from GiftWrap: {}", e);
                                            }
                                        }
                                        Err(e) => warn!("Rejected job from GiftWrap: {}", e),
                                    }
                                }
                            }
                        }
                    }
                    Ok(false)
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
