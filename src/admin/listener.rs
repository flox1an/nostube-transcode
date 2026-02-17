//! Admin command listener.
//!
//! Subscribes to kind 24207 ephemeral events (NIP-44 encrypted)
//! and processes admin commands using NIP-46-style RPC format.

use crate::admin::commands::{parse_request, AdminRequest, AdminResponseWire};
use crate::admin::handler::AdminHandler;
use crate::config::Config;
use crate::dvm_state::SharedDvmState;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

/// Admin RPC event kind (ephemeral range — relays don't store these)
const ADMIN_RPC_KIND: Kind = Kind::Custom(24207);

/// Starts listening for admin commands and processes them.
pub async fn run_admin_listener(
    client: Client,
    keys: Keys,
    state: SharedDvmState,
    config: Arc<Config>,
    config_notify: Arc<Notify>,
) {
    let handler = AdminHandler::new(state.clone(), client.clone(), config, config_notify);

    // Subscribe to kind 24207 events addressed to us
    let filter = Filter::new()
        .kind(ADMIN_RPC_KIND)
        .pubkey(keys.public_key())
        .since(Timestamp::now());

    // Wait for at least one relay to connect before subscribing
    let mut connected = false;
    for _i in 0..20 {
        let relays = client.relays().await;
        for relay in relays.values() {
            if relay.is_connected().await {
                connected = true;
                break;
            }
        }
        if connected {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    if !connected {
        warn!("Starting admin subscription without any connected relays");
    }

    // Try to subscribe with retries
    let mut subscribed = false;
    for i in 0..5 {
        match client.subscribe(vec![filter.clone()], None).await {
            Ok(_) => {
                subscribed = true;
                break;
            }
            Err(e) => {
                warn!("Admin subscription attempt {} failed: {}. Retrying...", i + 1, e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    }

    if !subscribed {
        error!("Failed to subscribe to admin events after multiple attempts");
        return;
    }

    info!("Listening for admin commands (kind 24207)...");

    // Handle incoming events
    client
        .handle_notifications(|notification| async {
            if let RelayPoolNotification::Event { event, .. } = notification {
                if event.kind == ADMIN_RPC_KIND {
                    handle_admin_event(&event, &keys, &handler, &client).await;
                }
            }
            Ok(false) // Continue listening
        })
        .await
        .ok();
}

async fn handle_admin_event(
    event: &Event,
    keys: &Keys,
    handler: &AdminHandler,
    client: &Client,
) {
    // Decrypt NIP-44 content
    let content = match nip44::decrypt(keys.secret_key(), &event.pubkey, &event.content) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to decrypt admin event: {}", e);
            return;
        }
    };

    // Parse v2 request format
    let request: AdminRequest = match parse_request(&content) {
        Ok(req) => req,
        Err(e) => {
            debug!("Failed to parse admin request: {}", e);
            return;
        }
    };

    let request_id = request.id.clone();

    // Convert to internal command
    let command = match request.to_command() {
        Ok(cmd) => {
            info!(
                "Received admin command from {}: {:?}",
                event.pubkey.to_bech32().unwrap_or_default(),
                cmd
            );
            cmd
        }
        Err(e) => {
            debug!("Unknown admin method: {}", e);
            // Send error response for unknown method
            let wire = AdminResponseWire {
                id: request_id,
                result: None,
                error: Some(e),
            };
            if let Ok(json) = serde_json::to_string(&wire) {
                if let Err(e) = send_admin_response(client, keys, &event.pubkey, &json).await {
                    error!("Failed to send error response: {}", e);
                }
            }
            return;
        }
    };

    // Process command
    let response = handler.handle(command, event.pubkey).await;

    // Wrap in v2 wire format
    let wire = AdminResponseWire::from_response(request_id, response);
    let response_json = match serde_json::to_string(&wire) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize response: {}", e);
            return;
        }
    };

    // Encrypt and send reply
    if let Err(e) = send_admin_response(client, keys, &event.pubkey, &response_json).await {
        error!("Failed to send response: {}", e);
    }
}

async fn send_admin_response(
    client: &Client,
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let encrypted = nip44::encrypt(
        keys.secret_key(),
        recipient,
        content,
        nip44::Version::default(),
    )?;

    let tags = vec![Tag::public_key(*recipient)];
    let event = EventBuilder::new(ADMIN_RPC_KIND, encrypted, tags).to_event(keys)?;

    client.send_event(event).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::commands::AdminCommand;

    #[test]
    fn test_parse_v2_request() {
        let json = r#"{"id":"abc","method":"status","params":{}}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.id, "abc");
        let cmd = req.to_command().unwrap();
        assert!(matches!(cmd, AdminCommand::Status));
    }

    #[test]
    fn test_parse_v2_request_invalid() {
        let result = parse_request("not json");
        assert!(result.is_err());
    }
}
