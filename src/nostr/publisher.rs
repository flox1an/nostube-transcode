use nostr_sdk::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, warn};

use crate::config::Config;
use crate::dvm_state::SharedDvmState;
use crate::error::DvmError;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

pub struct EventPublisher {
    config: Arc<Config>,
    client: Client,
    state: SharedDvmState,
}

impl EventPublisher {
    pub fn new(config: Arc<Config>, client: Client, state: SharedDvmState) -> Self {
        Self {
            config,
            client,
            state,
        }
    }

    /// Get current DVM operation relay URLs from shared state.
    async fn dvm_relay_urls(&self) -> Vec<String> {
        let state = self.state.read().await;
        state.config.relays.clone()
    }

    /// Ensure relay URLs are in the client pool and connected.
    pub async fn ensure_relays_connected(&self, relays: &[::url::Url]) {
        let mut added = false;
        for relay in relays {
            if self.client.add_relay(relay.as_str()).await.is_ok() {
                added = true;
            }
        }
        if added {
            self.client.connect().await;
        }
    }

    /// Publish an event to DVM config relays only.
    ///
    /// Used for announcements and other non-job-specific events.
    pub async fn publish(&self, builder: EventBuilder) -> Result<EventId, DvmError> {
        let relays = self.dvm_relay_urls().await;
        self.send_to(builder, &relays).await
    }

    /// Publish an event to DVM config relays + job-specific relays.
    ///
    /// Used for status updates, results, and other job-related events.
    pub async fn publish_for_job(
        &self,
        builder: EventBuilder,
        job_relays: &[::url::Url],
    ) -> Result<EventId, DvmError> {
        let mut relays = self.dvm_relay_urls().await;
        for r in job_relays {
            let s = r.as_str().trim_end_matches('/').to_string();
            if !relays
                .iter()
                .any(|existing| existing.trim_end_matches('/') == s)
            {
                relays.push(r.to_string());
            }
        }
        self.send_to(builder, &relays).await
    }

    /// Send an event to specific relay URLs with retries.
    async fn send_to(
        &self,
        builder: EventBuilder,
        relay_urls: &[String],
    ) -> Result<EventId, DvmError> {
        let event = builder
            .to_event(&self.config.nostr_keys)
            .map_err(|e| DvmError::JobRejected(format!("Failed to sign event: {}", e)))?;

        let event_id = event.id;

        if relay_urls.is_empty() {
            warn!(event_id = %event_id, "No relays configured, event not sent");
            return Ok(event_id);
        }

        // Ensure all relay URLs are in the client pool before sending
        let mut added = false;
        for url in relay_urls {
            if self.client.add_relay(url.as_str()).await.is_ok() {
                added = true;
            }
        }
        if added {
            self.client.connect().await;
        }

        for attempt in 1..=MAX_RETRIES {
            match self
                .client
                .send_event_to(relay_urls.iter().map(|s| s.as_str()), event.clone())
                .await
            {
                Ok(output) => {
                    debug!(
                        event_id = %event_id,
                        success_count = output.success.len(),
                        failed_count = output.failed.len(),
                        "Event published"
                    );
                    return Ok(event_id);
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        warn!(
                            event_id = %event_id,
                            attempt = attempt,
                            error = %e,
                            "Publish failed, retrying..."
                        );
                        sleep(Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                    } else {
                        error!(
                            event_id = %event_id,
                            error = %e,
                            "Publish failed after all retries"
                        );
                        return Err(e.into());
                    }
                }
            }
        }

        unreachable!()
    }
}
