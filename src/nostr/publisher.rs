use nostr_sdk::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, warn};

use crate::config::Config;
use crate::error::DvmError;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

pub struct EventPublisher {
    config: Arc<Config>,
    client: Client,
}

impl EventPublisher {
    pub fn new(config: Arc<Config>, client: Client) -> Self {
        Self { config, client }
    }

    /// Publish an event with retries
    pub async fn publish(&self, builder: EventBuilder) -> Result<EventId, DvmError> {
        let event = builder
            .to_event(&self.config.nostr_keys)
            .map_err(|e| DvmError::JobRejected(format!("Failed to sign event: {}", e)))?;

        let event_id = event.id;

        for attempt in 1..=MAX_RETRIES {
            match self.client.send_event(event.clone()).await {
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

    /// Publish an event to specific relays
    pub async fn publish_to_relays(
        &self,
        builder: EventBuilder,
        relays: &[::url::Url],
    ) -> Result<EventId, DvmError> {
        if relays.is_empty() {
            return self.publish(builder).await;
        }

        let event = builder
            .to_event(&self.config.nostr_keys)
            .map_err(|e| DvmError::JobRejected(format!("Failed to sign event: {}", e)))?;

        let event_id = event.id;

        // Add temporary relays if needed
        for relay in relays {
            let _ = self.client.add_relay(relay.as_str()).await;
        }

        // Publish
        let result = self.client.send_event(event).await?;

        debug!(
            event_id = %event_id,
            success_count = result.success.len(),
            "Event published to specific relays"
        );

        Ok(event_id)
    }
}
