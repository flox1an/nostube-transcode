//! Admin DM listener.
//!
//! Subscribes to encrypted DMs from the admin and processes commands.

use crate::admin::commands::{parse_command, serialize_response};
use crate::admin::handler::AdminHandler;
use crate::dvm_state::SharedDvmState;
use crate::pairing::PairingState;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// NIP-04 encrypted direct message kind
const ENCRYPTED_DM_KIND: Kind = Kind::Custom(4);

/// Starts listening for admin DMs and processes commands.
pub async fn run_admin_listener(
    client: Client,
    keys: Keys,
    state: SharedDvmState,
    pairing: Arc<RwLock<Option<PairingState>>>,
) {
    let handler = AdminHandler::new(state.clone(), client.clone(), pairing);

    // Subscribe to encrypted DMs (NIP-04) addressed to us
    let filter = Filter::new()
        .kind(ENCRYPTED_DM_KIND)
        .pubkey(keys.public_key())
        .since(Timestamp::now());

    if let Err(e) = client.subscribe(vec![filter], None).await {
        error!("Failed to subscribe to DMs: {}", e);
        return;
    }

    info!("Listening for admin DMs...");

    // Handle incoming events
    client
        .handle_notifications(|notification| async {
            if let RelayPoolNotification::Event { event, .. } = notification {
                if event.kind == ENCRYPTED_DM_KIND {
                    handle_dm(&event, &keys, &handler, &client).await;
                }
            }
            Ok(false) // Continue listening
        })
        .await
        .ok();
}

async fn handle_dm(event: &Event, keys: &Keys, handler: &AdminHandler, client: &Client) {
    // Decrypt NIP-04 message
    let content = match nip04::decrypt(keys.secret_key(), &event.pubkey, &event.content) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to decrypt DM: {}", e);
            return;
        }
    };

    // Parse command
    let command = match parse_command(&content) {
        Ok(cmd) => {
            info!(
                "Received admin command from {}: {:?}",
                event.pubkey.to_bech32().unwrap_or_default(),
                cmd
            );
            cmd
        }
        Err(e) => {
            debug!("Failed to parse command: {}", e);
            return;
        }
    };

    // Process command
    let response = handler.handle(command, event.pubkey).await;

    // Send response as encrypted DM
    let response_json = match serialize_response(&response) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize response: {}", e);
            return;
        }
    };

    // Encrypt and send reply
    if let Err(e) = send_encrypted_dm(client, keys, &event.pubkey, &response_json).await {
        error!("Failed to send response: {}", e);
    }
}

async fn send_encrypted_dm(
    client: &Client,
    keys: &Keys,
    recipient: &PublicKey,
    content: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let encrypted = nip04::encrypt(keys.secret_key(), recipient, content)?;

    // Build NIP-04 encrypted DM event
    // Tags: p-tag for recipient
    let tags = vec![Tag::public_key(*recipient)];
    let event = EventBuilder::new(ENCRYPTED_DM_KIND, encrypted, tags).to_event(keys)?;

    client.send_event(event).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::commands::AdminCommand;

    #[test]
    fn test_parse_valid_command() {
        let json = r#"{"cmd": "status"}"#;
        let cmd = parse_command(json).unwrap();
        assert!(matches!(cmd, AdminCommand::Status));
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_command("not json");
        assert!(result.is_err());
    }
}
