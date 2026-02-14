# Admin Protocol v2

Ephemeral RPC protocol for remote DVM administration over Nostr.

## Overview

- **Event kind:** `24207` (ephemeral range — relays do not store these)
- **Encryption:** NIP-44
- **Format:** NIP-46-style RPC (`method`/`params` request, `result`/`error` response)
- **Correlation:** Random `id` in request, echoed in response

Both requests and responses use kind 24207. Direction is determined by the `p`-tag (recipient) and `pubkey` (sender).

## Event Structure

### Request (admin → DVM)

```json
{
  "kind": 24207,
  "pubkey": "<admin_pubkey>",
  "created_at": 1234567890,
  "content": "<nip44_encrypted_json>",
  "tags": [["p", "<dvm_pubkey>"]],
  "id": "<event_id>",
  "sig": "<signature>"
}
```

Decrypted content:

```json
{
  "id": "a1b2c3d4e5f6...",
  "method": "get_config",
  "params": {}
}
```

### Response (DVM → admin)

```json
{
  "kind": 24207,
  "pubkey": "<dvm_pubkey>",
  "created_at": 1234567890,
  "content": "<nip44_encrypted_json>",
  "tags": [["p", "<admin_pubkey>"]],
  "id": "<event_id>",
  "sig": "<signature>"
}
```

Decrypted content (success):

```json
{
  "id": "a1b2c3d4e5f6...",
  "result": { ... }
}
```

Decrypted content (error):

```json
{
  "id": "a1b2c3d4e5f6...",
  "error": "Unauthorized"
}
```

`result` and `error` are mutually exclusive. The `id` always matches the request.

## Method Reference

| Method | Params | Result |
|---|---|---|
| `claim_admin` | `{"secret": "xxxx-xxxx-xxxx"}` | `{"msg": "Admin claimed"}` |
| `get_config` | `{}` | `ConfigResponse` |
| `set_relays` | `{"relays": ["wss://..."]}` | `ConfigResponse` |
| `set_blossom_servers` | `{"servers": ["https://..."]}` | `ConfigResponse` |
| `set_blob_expiration` | `{"days": 30}` | `ConfigResponse` |
| `set_profile` | `{"name": "...", "about": "..."}` | `ConfigResponse` |
| `set_config` | `{"relays?": [...], "blossom_servers?": [...], "blob_expiration_days?": N, "name?": "...", "about?": "..."}` | `ConfigResponse` |
| `pause` | `{}` | `StatusResponse` |
| `resume` | `{}` | `StatusResponse` |
| `status` | `{}` | `StatusResponse` |
| `job_history` | `{"limit?": 20}` | `JobHistoryResponse` |
| `get_dashboard` | `{"limit?": 20}` | `DashboardResponse` |
| `self_test` | `{}` | `SelfTestResponse` |
| `system_info` | `{}` | `SystemInfoResponse` |
| `import_env_config` | `{}` | `ConfigResponse` |

### Response Shapes

**ConfigResponse:**
```json
{"config": {"relays": [...], "blossom_servers": [...], "blob_expiration_days": 30, "name": "...", "about": "...", "paused": false}}
```

**StatusResponse:**
```json
{"paused": false, "jobs_active": 0, "jobs_completed": 5, "jobs_failed": 1, "uptime_secs": 3600, "hwaccel": "videotoolbox", "version": "0.1.0"}
```

**DashboardResponse:**
```json
{"status": {<StatusResponse>}, "config": {<ConfigData>}, "jobs": [{<JobInfo>}, ...]}
```

**JobHistoryResponse:**
```json
{"jobs": [{"id": "...", "status": "completed", "input_url": "...", "output_url": "...", "started_at": "...", "completed_at": "...", "duration_secs": 42}]}
```

## Subscription Filters

**Admin subscribes to DVM responses:**
```json
{"kinds": [24207], "authors": ["<dvm_pubkey>"], "#p": ["<admin_pubkey>"], "since": <now>}
```

**DVM subscribes to admin commands:**
```json
{"kinds": [24207], "#p": ["<dvm_pubkey>"], "since": <now>}
```

## Migration from v1

v1 used NIP-04 encrypted DMs (kind 4) with a flat command format.

| Aspect | v1 | v2 |
|---|---|---|
| Event kind | `4` (DM) | `24207` (ephemeral) |
| Encryption | NIP-04 | NIP-44 |
| Storage | Relays store events | Relays discard (ephemeral) |
| Request format | `{"cmd": "get_config", ...params}` | `{"id": "...", "method": "get_config", "params": {...}}` |
| Response format | `{"ok": true, "error": "...", ...data}` | `{"id": "...", "result": {...}}` or `{"id": "...", "error": "..."}` |
| Signer API | `signer.nip04.encrypt/decrypt` | `signer.nip44.encrypt/decrypt` |
| Expiration tag | `["expiration", "..."]` | Not needed (ephemeral) |

### Migration Checklist

1. Change event kind from `4` to `24207`
2. Switch encryption from NIP-04 to NIP-44
3. Wrap requests as `{id, method, params}` instead of `{cmd, ...fields}`
4. Parse responses as `{id, result?, error?}` instead of `{ok, error?, ...data}`
5. Generate a random hex `id` per request for correlation
6. Update subscription filters to kind `24207`
7. Remove expiration tags (unnecessary for ephemeral events)
8. Check signer for `nip44` support instead of `nip04`
