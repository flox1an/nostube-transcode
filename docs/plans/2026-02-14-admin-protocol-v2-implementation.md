# Admin Protocol v2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate admin communication from NIP-04 DMs (kind 4) to ephemeral NIP-44 events (kind 24207) with NIP-46-style RPC format.

**Architecture:** Add wire types (`AdminRequest`/`AdminResponseWire`) that bridge between the NIP-46-style JSON format and the existing internal `AdminCommand`/`AdminResponse` types. The handler stays untouched. Listener switches encryption and event kind. Frontend mirrors the same changes.

**Tech Stack:** Rust (nostr-sdk 0.35, nip44), TypeScript (nostr-tools, applesauce-signers)

---

### Task 1: Add wire types and parsing to `src/admin/commands.rs`

**Files:**
- Modify: `src/admin/commands.rs`

**Step 1: Write failing tests for the new wire format**

Add these tests at the end of the existing `mod tests` block in `src/admin/commands.rs`:

```rust
#[test]
fn test_parse_admin_request_get_config() {
    let json = r#"{"id":"abc123","method":"get_config","params":{}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.id, "abc123");
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::GetConfig);
}

#[test]
fn test_parse_admin_request_set_relays() {
    let json = r#"{"id":"def456","method":"set_relays","params":{"relays":["wss://relay1.example.com"]}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::SetRelays { relays: vec!["wss://relay1.example.com".to_string()] });
}

#[test]
fn test_parse_admin_request_claim_admin() {
    let json = r#"{"id":"ghi789","method":"claim_admin","params":{"secret":"abc1-def2-ghi3"}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::ClaimAdmin { secret: "abc1-def2-ghi3".to_string() });
}

#[test]
fn test_parse_admin_request_set_profile() {
    let json = r#"{"id":"x","method":"set_profile","params":{"name":"My DVM","about":"A test"}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::SetProfile { name: Some("My DVM".to_string()), about: Some("A test".to_string()) });
}

#[test]
fn test_parse_admin_request_job_history_default() {
    let json = r#"{"id":"x","method":"job_history","params":{}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::JobHistory { limit: 20 });
}

#[test]
fn test_parse_admin_request_job_history_with_limit() {
    let json = r#"{"id":"x","method":"job_history","params":{"limit":50}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::JobHistory { limit: 50 });
}

#[test]
fn test_parse_admin_request_set_config() {
    let json = r#"{"id":"x","method":"set_config","params":{"relays":["wss://r.com"],"name":"New"}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let cmd = req.to_command().unwrap();
    assert_eq!(cmd, AdminCommand::SetConfig {
        relays: Some(vec!["wss://r.com".to_string()]),
        blossom_servers: None,
        blob_expiration_days: None,
        name: Some("New".to_string()),
        about: None,
    });
}

#[test]
fn test_parse_admin_request_unknown_method() {
    let json = r#"{"id":"x","method":"unknown_thing","params":{}}"#;
    let req: AdminRequest = serde_json::from_str(json).unwrap();
    let result = req.to_command();
    assert!(result.is_err());
}

#[test]
fn test_serialize_response_wire_success() {
    let wire = AdminResponseWire::from_response("abc123".to_string(), AdminResponse::ok_with_msg("done"));
    let json = serde_json::to_string(&wire).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["id"], "abc123");
    assert!(parsed["result"].is_object());
    assert!(parsed.get("error").is_none() || parsed["error"].is_null());
}

#[test]
fn test_serialize_response_wire_error() {
    let wire = AdminResponseWire::from_response("abc123".to_string(), AdminResponse::error("bad"));
    let json = serde_json::to_string(&wire).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["id"], "abc123");
    assert_eq!(parsed["error"], "bad");
    assert!(parsed.get("result").is_none() || parsed["result"].is_null());
}

#[test]
fn test_serialize_response_wire_with_data() {
    let config_data = ConfigData {
        relays: vec!["wss://r.com".to_string()],
        blossom_servers: vec![],
        blob_expiration_days: 30,
        name: None,
        about: None,
        paused: false,
    };
    let response = AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse { config: config_data }));
    let wire = AdminResponseWire::from_response("id1".to_string(), response);
    let json = serde_json::to_string(&wire).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["id"], "id1");
    assert!(parsed["result"]["config"]["relays"][0] == "wss://r.com");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib admin::commands::tests -- --nocapture 2>&1 | head -50`
Expected: Compilation errors — `AdminRequest` and `AdminResponseWire` don't exist yet.

**Step 3: Implement `AdminRequest` and `AdminResponseWire`**

Add to `src/admin/commands.rs`, after the existing `AdminCommand` enum and before `AdminResponse`:

```rust
/// Wire format for incoming admin requests (NIP-46 style).
///
/// Decrypted from NIP-44 content of kind 24207 events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRequest {
    /// Random correlation ID, echoed in response
    pub id: String,
    /// Command method name
    pub method: String,
    /// Command parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

impl AdminRequest {
    /// Convert wire request to internal AdminCommand.
    pub fn to_command(&self) -> Result<AdminCommand, String> {
        match self.method.as_str() {
            "claim_admin" => {
                let secret: String = serde_json::from_value(
                    self.params.get("secret").cloned().unwrap_or_default()
                ).map_err(|e| format!("invalid params: {}", e))?;
                Ok(AdminCommand::ClaimAdmin { secret })
            }
            "get_config" => Ok(AdminCommand::GetConfig),
            "set_relays" => {
                let relays: Vec<String> = serde_json::from_value(
                    self.params.get("relays").cloned().unwrap_or_default()
                ).map_err(|e| format!("invalid params: {}", e))?;
                Ok(AdminCommand::SetRelays { relays })
            }
            "set_blossom_servers" => {
                let servers: Vec<String> = serde_json::from_value(
                    self.params.get("servers").cloned().unwrap_or_default()
                ).map_err(|e| format!("invalid params: {}", e))?;
                Ok(AdminCommand::SetBlossomServers { servers })
            }
            "set_blob_expiration" => {
                let days: u32 = serde_json::from_value(
                    self.params.get("days").cloned().unwrap_or_default()
                ).map_err(|e| format!("invalid params: {}", e))?;
                Ok(AdminCommand::SetBlobExpiration { days })
            }
            "set_profile" => {
                let name: Option<String> = self.params.get("name")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let about: Option<String> = self.params.get("about")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                Ok(AdminCommand::SetProfile { name, about })
            }
            "pause" => Ok(AdminCommand::Pause),
            "resume" => Ok(AdminCommand::Resume),
            "status" => Ok(AdminCommand::Status),
            "job_history" => {
                let limit: u32 = self.params.get("limit")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or(20);
                Ok(AdminCommand::JobHistory { limit })
            }
            "get_dashboard" => {
                let limit: u32 = self.params.get("limit")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or(20);
                Ok(AdminCommand::GetDashboard { limit })
            }
            "set_config" => {
                let relays: Option<Vec<String>> = self.params.get("relays")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let blossom_servers: Option<Vec<String>> = self.params.get("blossom_servers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let blob_expiration_days: Option<u32> = self.params.get("blob_expiration_days")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let name: Option<String> = self.params.get("name")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let about: Option<String> = self.params.get("about")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                Ok(AdminCommand::SetConfig { relays, blossom_servers, blob_expiration_days, name, about })
            }
            "self_test" => Ok(AdminCommand::SelfTest),
            "system_info" => Ok(AdminCommand::SystemInfo),
            "import_env_config" => Ok(AdminCommand::ImportEnvConfig),
            other => Err(format!("unknown method: {}", other)),
        }
    }
}
```

Add after the `AdminResponse` impl block:

```rust
/// Wire format for outgoing admin responses (NIP-46 style).
///
/// Encrypted with NIP-44 and sent as kind 24207 event content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminResponseWire {
    /// Correlation ID from the request
    pub id: String,
    /// Result data (present on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error message (present on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AdminResponseWire {
    /// Convert internal AdminResponse to wire format.
    pub fn from_response(id: String, response: AdminResponse) -> Self {
        if !response.ok {
            return Self {
                id,
                result: None,
                error: response.error,
            };
        }

        // Build result value from msg and/or data
        let result = if let Some(data) = response.data {
            serde_json::to_value(data).ok()
        } else if let Some(msg) = response.msg {
            Some(serde_json::json!({ "msg": msg }))
        } else {
            Some(serde_json::json!({}))
        };

        Self {
            id,
            result,
            error: None,
        }
    }
}
```

Also update the public API functions. Replace the existing `parse_command` and `serialize_response`:

```rust
/// Parse an admin request from JSON (v2 wire format).
pub fn parse_request(json: &str) -> Result<AdminRequest, serde_json::Error> {
    serde_json::from_str(json)
}

/// Serialize an admin response to JSON (v2 wire format).
pub fn serialize_response_wire(id: String, response: &AdminResponse) -> Result<String, serde_json::Error> {
    let wire = AdminResponseWire::from_response(id, response.clone());
    serde_json::to_string(&wire)
}
```

Keep the old `parse_command` and `serialize_response` functions — they're still used by existing tests and may be useful internally. Just add the new ones alongside.

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib admin::commands::tests -- --nocapture`
Expected: All tests pass, including the new ones.

**Step 5: Commit**

```bash
git add src/admin/commands.rs
git commit -m "feat: add AdminRequest/AdminResponseWire types for protocol v2"
```

---

### Task 2: Update listener to use kind 24207 + NIP-44

**Files:**
- Modify: `src/admin/listener.rs`

**Step 1: Write failing test for request parsing in listener**

Add to the `mod tests` block at the end of `src/admin/listener.rs`:

```rust
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
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib admin::listener::tests -- --nocapture`
Expected: Fails because `parse_request` is not imported yet.

**Step 3: Update listener implementation**

Replace the full content of `src/admin/listener.rs`:

```rust
//! Admin command listener.
//!
//! Subscribes to kind 24207 ephemeral events (NIP-44 encrypted)
//! and processes admin commands using NIP-46-style RPC format.

use crate::admin::commands::{parse_request, AdminRequest, AdminResponseWire};
use crate::admin::handler::AdminHandler;
use crate::config::Config;
use crate::dvm_state::SharedDvmState;
use crate::pairing::PairingState;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Admin RPC event kind (ephemeral range — relays don't store these)
const ADMIN_RPC_KIND: Kind = Kind::Custom(24207);

/// Starts listening for admin commands and processes them.
pub async fn run_admin_listener(
    client: Client,
    keys: Keys,
    state: SharedDvmState,
    pairing: Arc<RwLock<Option<PairingState>>>,
    config: Arc<Config>,
) {
    let handler = AdminHandler::new(state.clone(), client.clone(), pairing, config);

    // Subscribe to kind 24207 events addressed to us
    let filter = Filter::new()
        .kind(ADMIN_RPC_KIND)
        .pubkey(keys.public_key())
        .since(Timestamp::now());

    if let Err(e) = client.subscribe(vec![filter], None).await {
        error!("Failed to subscribe to admin events: {}", e);
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
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib admin -- --nocapture`
Expected: All admin tests pass.

**Step 5: Run `cargo check` to verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors. There may be warnings about unused `parse_command`/`serialize_response` — that's fine.

**Step 6: Commit**

```bash
git add src/admin/listener.rs
git commit -m "feat: switch admin listener to kind 24207 + NIP-44"
```

---

### Task 3: Update frontend admin module

**Files:**
- Modify: `frontend/src/nostr/admin.ts`

**Step 1: Update the admin module**

Replace `sendAdminCommand` and `subscribeToAdminResponses` in `frontend/src/nostr/admin.ts`:

Remove the old `AdminCommand` type and `ADMIN_COMMAND_EXPIRATION_SECS` constant. Replace with:

```typescript
/** Admin RPC event kind (ephemeral) */
const ADMIN_RPC_KIND = 24207;

/** Generate a random hex ID for request correlation */
function randomId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

// Admin request (NIP-46 style wire format)
export interface AdminRequest {
  id: string;
  method: string;
  params: Record<string, unknown>;
}

// Admin response (NIP-46 style wire format)
export interface AdminResponseWire {
  id: string;
  result?: unknown;
  error?: string;
}
```

Update `sendAdminCommand`:

```typescript
export async function sendAdminCommand(
  signer: ISigner,
  dvmPubkey: string,
  method: string,
  params: Record<string, unknown>,
  relays: string[]
): Promise<string> {
  if (!signer.nip44) {
    throw new Error("Signer does not support NIP-44 encryption");
  }

  const id = randomId();
  const adminPubkey = await signer.getPublicKey();
  const request: AdminRequest = { id, method, params };
  const content = JSON.stringify(request);
  const encrypted = await signer.nip44.encrypt(dvmPubkey, content);

  const now = Math.floor(Date.now() / 1000);

  const template: Event = {
    kind: ADMIN_RPC_KIND,
    pubkey: adminPubkey,
    created_at: now,
    tags: [["p", dvmPubkey]],
    content: encrypted,
    id: "",
    sig: "",
  };

  const signedEvent = await signer.signEvent(template);
  await relayPool.publish(relays, signedEvent);
  return id;
}
```

Update `subscribeToAdminResponses`:

```typescript
export function subscribeToAdminResponses(
  signer: ISigner,
  adminPubkey: string,
  dvmPubkey: string,
  relays: string[],
  onResponse: (response: AdminResponseWire) => void
): () => void {
  const filters = {
    kinds: [ADMIN_RPC_KIND],
    authors: [dvmPubkey],
    "#p": [adminPubkey],
    since: Math.floor(Date.now() / 1000),
  };

  const subscription = relayPool
    .subscription(relays, filters)
    .pipe(
      filter((response): response is Event =>
        typeof response !== "string" && "kind" in response
      ),
      mapEventsToStore(eventStore, true)
    )
    .subscribe({
      async next(event) {
        if (event.kind !== ADMIN_RPC_KIND) return;

        try {
          if (!signer.nip44) {
            console.error("Signer does not support NIP-44");
            return;
          }

          const decrypted = await signer.nip44.decrypt(
            dvmPubkey,
            event.content
          );
          const response = JSON.parse(decrypted) as AdminResponseWire;
          onResponse(response);
        } catch (e) {
          console.error("Failed to decrypt admin response:", e);
        }
      },
    });

  return () => subscription.unsubscribe();
}
```

Remove the old `AdminResponse` interface (replaced by `AdminResponseWire`). Keep all the existing data interfaces (`DvmConfig`, `DvmStatus`, `DvmDashboard`, `DvmJob`, `SelfTestResult`, `SystemInfoResult`, etc.) — those represent the `result` payload shapes.

**Step 2: Run lint to verify no syntax errors**

Run: `cd frontend && npx tsc --noEmit 2>&1 | head -30`
Expected: Type errors in components that use the old API — this is expected, we fix them in the next task.

**Step 3: Commit**

```bash
git add frontend/src/nostr/admin.ts
git commit -m "feat: switch frontend admin module to kind 24207 + NIP-44"
```

---

### Task 4: Update frontend components for new response format

**Files:**
- Modify: `frontend/src/components/DvmDetailPanel.tsx`
- Modify: `frontend/src/components/SelfTest.tsx`
- Modify: `frontend/src/components/PairDvmModal.tsx`
- Modify: `frontend/src/components/DvmList.tsx`

These components need two changes:
1. Call `sendAdminCommand(signer, pubkey, method, params, relays)` instead of `sendAdminCommand(signer, pubkey, {cmd, ...}, relays)`
2. Handle `AdminResponseWire` (with `result`/`error`) instead of `AdminResponse` (with `ok`/`error`/flattened data)

**Step 1: Update DvmDetailPanel.tsx**

Change the import:
```typescript
// Old
import { sendAdminCommand, subscribeToAdminResponses, type AdminResponse, ... } from "../nostr/admin";
// New
import { sendAdminCommand, subscribeToAdminResponses, type AdminResponseWire, ... } from "../nostr/admin";
```

Update `handleAdminResponse` callback — change from duck-typing `response.ok` + `"status" in response` to parsing `response.result`:

```typescript
const handleAdminResponse = useCallback((response: AdminResponseWire) => {
    if (response.error) {
      console.error("Admin command failed:", response.error);
      return;
    }

    const data = response.result as Record<string, unknown>;
    if (!data) return;

    // Dashboard response (status + config + jobs)
    if ("status" in data && "config" in data && "jobs" in data) {
      const dashboard = data as unknown as DvmDashboard;
      setStatus(dashboard.status);
      setConfig(dashboard.config);
      setConfigForm(dashboard.config);
      setJobs(dashboard.jobs);
    }
    // Status response (from status, pause, or resume commands)
    else if ("paused" in data && "jobs_active" in data) {
      setStatus(data as unknown as DvmStatus);
    }
    // Config response (from set_config)
    else if ("config" in data) {
      const cfg = (data as { config: DvmConfig }).config;
      setConfig(cfg);
      setConfigForm(cfg);
    }
    // Job history response
    else if ("jobs" in data) {
      setJobs((data as { jobs: DvmJob[] }).jobs);
    }
  }, []);
```

Update all `sendAdminCommand` calls — change from object `{cmd: "x", ...}` to `method, params`:

```typescript
// Old: sendAdminCommand(signer, dvm.pubkey, { cmd: "get_dashboard", limit: 20 }, RELAYS)
// New:
sendAdminCommand(signer, dvm.pubkey, "get_dashboard", { limit: 20 }, RELAYS)

// Old: sendAdminCommand(signer, dvm.pubkey, { cmd: "pause" }, RELAYS)
// New:
sendAdminCommand(signer, dvm.pubkey, "pause", {}, RELAYS)

// Old: sendAdminCommand(signer, dvm.pubkey, { cmd: "resume" }, RELAYS)
// New:
sendAdminCommand(signer, dvm.pubkey, "resume", {}, RELAYS)

// Old: sendAdminCommand(signer, dvm.pubkey, { cmd: "set_config", relays: ..., ... }, RELAYS)
// New:
sendAdminCommand(signer, dvm.pubkey, "set_config", {
  relays: configForm.relays,
  blossom_servers: configForm.blossom_servers,
  blob_expiration_days: configForm.blob_expiration_days,
  name: configForm.name,
  about: configForm.about,
}, RELAYS)
```

**Step 2: Update SelfTest.tsx**

Same pattern — change import, response handler, and `sendAdminCommand` calls:

```typescript
// Import change
import { sendAdminCommand, subscribeToAdminResponses, type AdminResponseWire, ... } from "../nostr/admin";

// Response handler — check response.error instead of response.ok
const handleResponse = useCallback((response: AdminResponseWire) => {
  if (response.error) {
    // handle error
    return;
  }
  const data = response.result as Record<string, unknown>;
  // ... parse system_info or self_test result from data
}, [...]);

// Command calls
// Old: sendAdminCommand(signer, dvmPubkey, { cmd: "system_info" }, RELAYS)
// New:
sendAdminCommand(signer, dvmPubkey, "system_info", {}, RELAYS)

// Old: sendAdminCommand(signer, dvmPubkey, { cmd: "self_test" }, RELAYS)
// New:
sendAdminCommand(signer, dvmPubkey, "self_test", {}, RELAYS)
```

**Step 3: Update PairDvmModal.tsx**

```typescript
// Import change
import { sendAdminCommand, subscribeToAdminResponses } from "../nostr/admin";

// Command call
// Old: sendAdminCommand(signer, dvmPubkey, { cmd: "claim_admin", secret: secret }, relays)
// New:
sendAdminCommand(signer, dvmPubkey, "claim_admin", { secret }, relays)

// Response handler — check response.error instead of response.ok
```

**Step 4: Update DvmList.tsx**

```typescript
// Import change
import { sendAdminCommand, subscribeToAdminResponses, ... } from "../nostr/admin";

// Command call
// Old: sendAdminCommand(signer, metadata.pubkey, { cmd: "status" }, RELAYS)
// New:
sendAdminCommand(signer, metadata.pubkey, "status", {}, RELAYS)

// Response handler — check response.error instead of response.ok, parse from response.result
```

**Step 5: Run TypeScript check**

Run: `cd frontend && npx tsc --noEmit 2>&1 | head -30`
Expected: No errors.

**Step 6: Run lint**

Run: `cd frontend && npm run lint 2>&1 | tail -10`
Expected: Clean or only pre-existing warnings.

**Step 7: Commit**

```bash
git add frontend/src/components/DvmDetailPanel.tsx frontend/src/components/SelfTest.tsx frontend/src/components/PairDvmModal.tsx frontend/src/components/DvmList.tsx
git commit -m "feat: update frontend components for admin protocol v2"
```

---

### Task 5: Write migration doc

**Files:**
- Create: `docs/admin-protocol.md`

**Step 1: Write the protocol spec and migration guide**

Create `docs/admin-protocol.md` with:
- Protocol overview (kind 24207, NIP-44, ephemeral)
- Event structure with full JSON examples for request and response
- Method reference table (all methods, params, response shapes)
- Migration checklist: kind 4→24207, NIP-04→NIP-44, `{cmd,...}`→`{id,method,params}`, `{ok,error,...}`→`{id,result,error}`

**Step 2: Commit**

```bash
git add docs/admin-protocol.md
git commit -m "docs: add admin protocol v2 spec and migration guide"
```

---

### Task 6: Clean up dead code and run full test suite

**Files:**
- Modify: `src/admin/commands.rs` (remove old `parse_command`/`serialize_response` if unused)
- Modify: `src/dvm/announcement.rs` (update "nip04" capability tag if present)

**Step 1: Check for remaining references to old functions**

Run: `rg 'parse_command|serialize_response' src/`
If `parse_command` and `serialize_response` are only used in tests within `commands.rs`, update those tests to use the new `parse_request` / `AdminRequest::to_command()` pattern and remove the old functions.

**Step 2: Check for "nip04" references in announcement**

The DVM announcement in `src/dvm/announcement.rs` may advertise "nip04" encryption support. Update or remove this if present.

**Step 3: Run full backend test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass.

**Step 4: Run frontend build**

Run: `cd frontend && npm run build 2>&1 | tail -10`
Expected: Build succeeds.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: clean up old NIP-04 admin code"
```
