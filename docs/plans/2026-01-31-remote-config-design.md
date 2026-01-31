# Remote Configuration Design

## Overview

This design enables zero-config DVM startup with full remote configuration via Nostr. The DVM stores only its identity locally; all configuration lives on Nostr as encrypted app-specific data. Operators manage their DVMs through a web app that communicates entirely over Nostr protocol.

## Goals

- **Zero config startup** - Run the binary with no configuration required
- **Remote management** - Configure and operate DVMs from anywhere via Nostr
- **Hot reload** - Configuration changes apply immediately without restart
- **Multi-DVM support** - Single operator can manage multiple DVMs from one dashboard
- **Minimal local state** - Only the identity key is stored locally

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Nostr Network                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   Relay 1   │  │   Relay 2   │  │   Relay 3   │                 │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                 │
│         │                │                │                         │
│         └────────────────┼────────────────┘                         │
│                          │                                          │
│    ┌─────────────────────┼─────────────────────┐                   │
│    │                     │                     │                    │
│    ▼                     ▼                     ▼                    │
│ ┌──────┐           ┌──────────┐          ┌──────────┐              │
│ │ DVM  │◄─────────►│  Admin   │◄────────►│  Users   │              │
│ │      │  DMs      │  Web App │  DMs     │  (jobs)  │              │
│ └──┬───┘           └──────────┘          └──────────┘              │
│    │                                                                │
│    │ Publishes:                                                     │
│    │ - kind 0 (profile)                                            │
│    │ - kind 10002 (relay list)                                     │
│    │ - kind 30078 (encrypted config)                               │
│    │ - kind 31990 (DVM announcement)                               │
│    │ - kind 7000 (job status)                                      │
│    │ - kind 6207 (job results)                                     │
└────┼────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────┐
│  Local Storage  │
│  identity.key   │
└─────────────────┘
```

## Local Storage

### Identity Key

**Location:**
- Linux/macOS: `~/.local/share/dvm-video/identity.key`
- Docker: `/data/identity.key` (volume mount)

**Format:** 64-character hex private key (nsec internally)

**Lifecycle:**
- Auto-generated on first startup if not present
- Never modified after creation
- Only local file the DVM requires

### Bootstrap Relays

**Source (in priority order):**
1. `BOOTSTRAP_RELAYS` environment variable (comma-separated)
2. Hardcoded fallbacks: `wss://relay.damus.io`, `wss://nos.lol`, `wss://relay.nostr.band`

**Purpose:** Initial connection before config is loaded. Replaced by configured relays after startup.

## First Run & Identity Setup

When the DVM starts with no existing identity:

```
1. Generate new keypair
2. Save private key to identity.key
3. Connect to bootstrap relays
4. Publish kind 0 profile:
   {
     "name": "Video Transform DVM",
     "about": "Unconfigured DVM - awaiting operator"
   }
5. Publish kind 10002 relay list (bootstrap relays)
6. Enter pairing mode (no NIP-78 config found)
```

## Admin Pairing

### Pairing Mode

When DVM has no admin configured, it enters pairing mode:

```
═══════════════════════════════════════════════════════════════
VIDEO TRANSFORM DVM - PAIRING MODE

DVM pubkey: npub1abc123...

Pair this DVM by opening:
https://dvm-admin.example.com/pair?dvm=npub1abc...&secret=8f3k-x9m2-p4wn

Or scan:
█████████████████████████████
█████████████████████████████
█████   █ █ ██ █   ██████████
█████ ███ █  █ ██ ███████████
█████████████████████████████
█████████████████████████████

Waiting for pairing request...
═══════════════════════════════════════════════════════════════
```

**Pairing URL base** configurable via `DVM_ADMIN_APP_URL` env var.

### Pairing Flow

```
┌──────────┐         ┌──────────┐         ┌──────────┐
│ Console  │         │ Web App  │         │   DVM    │
└────┬─────┘         └────┬─────┘         └────┬─────┘
     │                    │                    │
     │ Show pairing URL   │                    │
     │ + QR code          │                    │
     │                    │                    │
     │    User scans/clicks URL               │
     │ ──────────────────►│                    │
     │                    │                    │
     │                    │ User logs in       │
     │                    │ (NIP-07/46/nsec)   │
     │                    │                    │
     │                    │ Encrypted DM:      │
     │                    │ {"cmd":"claim_admin", "secret":"8f3k-x9m2-p4wn"}
     │                    │───────────────────►│
     │                    │                    │
     │                    │                    │ Verify secret
     │                    │                    │ Store admin pubkey
     │                    │                    │ in NIP-78 config
     │                    │                    │
     │                    │   {"ok": true}     │
     │                    │◄───────────────────│
     │                    │                    │
     │ "Admin paired:     │                    │ Publish kind 31990
     │  npub1xyz..."      │                    │ announcement
     │                    │                    │
     │                    │                    │ Start normal operation
```

### Pairing Security

- **One-time secret:** Invalidated after successful pairing or timeout (5 minutes)
- **Console access required:** Secret only visible via console/SSH
- **First claim wins:** No additional protection beyond secret
- **Re-pairing:** Requires console access to restart DVM in pairing mode

## Configuration Storage (NIP-78)

### Event Structure

```json
{
  "kind": 30078,
  "pubkey": "<dvm_pubkey>",
  "tags": [
    ["d", "video-dvm-config"]
  ],
  "content": "<NIP-44 encrypted JSON>",
  "created_at": 1234567890
}
```

### Config Schema

```json
{
  "version": 1,
  "admin": "npub1...",
  "relays": [
    "wss://relay.damus.io",
    "wss://nos.lol"
  ],
  "blossom_servers": [
    "https://blossom.example.com"
  ],
  "blob_expiration_days": 30,
  "name": "My Video DVM",
  "about": "Transforms videos to HLS with hardware acceleration",
  "paused": false
}
```

### Config Updates

1. Admin sends config command via encrypted DM
2. DVM validates and updates in-memory config
3. DVM publishes updated NIP-78 event (encrypted to self)
4. Side effects triggered:
   - Relays changed → reconnect to new relay set, update kind 10002
   - Name/about changed → update kind 0 profile, re-publish kind 31990
   - Blossom changed → update kind 10063 (user server list) if desired

## Admin Commands

All commands sent as NIP-44 encrypted DMs from admin to DVM.

### Command Format

**Request:**
```json
{"cmd": "<command_name>", ...params}
```

**Response:**
```json
{"ok": true/false, "error": "...", ...data}
```

### Configuration Commands

**Get current config:**
```json
{"cmd": "get_config"}
```
```json
{
  "ok": true,
  "config": {
    "relays": ["wss://..."],
    "blossom_servers": ["https://..."],
    "blob_expiration_days": 30,
    "name": "My DVM",
    "about": "Description",
    "paused": false
  }
}
```

**Set relays:**
```json
{"cmd": "set_relays", "relays": ["wss://relay1.com", "wss://relay2.com"]}
```
```json
{"ok": true}
```

**Set Blossom servers:**
```json
{"cmd": "set_blossom_servers", "servers": ["https://blossom1.com", "https://blossom2.com"]}
```
```json
{"ok": true}
```

**Set blob expiration:**
```json
{"cmd": "set_blob_expiration", "days": 30}
```
```json
{"ok": true}
```

**Set profile:**
```json
{"cmd": "set_profile", "name": "My Video DVM", "about": "Description here"}
```
```json
{"ok": true}
```

### Operational Commands

**Pause DVM (reject new jobs):**
```json
{"cmd": "pause"}
```
```json
{"ok": true, "msg": "DVM paused, rejecting new jobs"}
```

**Resume DVM:**
```json
{"cmd": "resume"}
```
```json
{"ok": true, "msg": "DVM resumed"}
```

**Get status:**
```json
{"cmd": "status"}
```
```json
{
  "ok": true,
  "paused": false,
  "jobs_active": 2,
  "jobs_completed": 47,
  "jobs_failed": 3,
  "uptime_secs": 3600,
  "hwaccel": "NVIDIA NVENC",
  "version": "0.1.0"
}
```

**Get job history:**
```json
{"cmd": "job_history", "limit": 20}
```
```json
{
  "ok": true,
  "jobs": [
    {
      "id": "event_id_1",
      "status": "completed",
      "input_url": "https://...",
      "output_url": "https://...",
      "started_at": 1234567890,
      "completed_at": 1234567900,
      "duration_secs": 10
    }
  ]
}
```

**Run self-test:**
```json
{"cmd": "self_test"}
```
```json
{
  "ok": true,
  "success": true,
  "video_duration_secs": 30.0,
  "encode_time_secs": 12.5,
  "speed_ratio": 2.4,
  "hwaccel": "NVIDIA NVENC",
  "resolution": "720p"
}
```

## DVM Announcement (Kind 31990)

### Event Structure

```json
{
  "kind": 31990,
  "pubkey": "<dvm_pubkey>",
  "tags": [
    ["d", "video-transform"],
    ["k", "5207"],
    ["p", "<admin_pubkey>", "", "operator"],
    ["name", "My Video DVM"],
    ["about", "Transforms videos to HLS with hardware acceleration"],
    ["nip90Params", "<supported_params_json>"]
  ],
  "content": "",
  "created_at": 1234567890
}
```

### Operator Tag

The `["p", "<admin_pubkey>", "", "operator"]` tag enables:
- Discovery: Web app queries `#p` to find DVMs a user operates
- Attribution: Public record of who runs the DVM

### Announcement Lifecycle

- **Published:** After successful pairing
- **Updated:** When name, about, or relay config changes
- **Deleted:** Not automatically (operator can manually delete if decommissioning)

## Web App Architecture

### Deployment Options

1. **Bundled with DVM** - Served on HTTP port (existing infrastructure)
2. **Hosted separately** - Static site on Vercel, GitHub Pages, Netlify
3. **Self-hosted** - Any static file server

### Technology

- Pure Nostr client (no backend API)
- Connects directly to relays
- All DVM communication via NIP-44 encrypted DMs

### Authentication

- NIP-07 browser extension (Alby, nos2x, etc.)
- NIP-46 Nostr Connect (Amber, nsecbunker)
- Direct nsec entry (with warnings)

### Views

**1. Login**
- Nostr authentication method selection
- Connection to user's preferred relays

**2. Pair New DVM**
- URL auto-parsed if opened via pairing link
- Manual entry: DVM pubkey + pairing secret
- Send claim command, show success/failure

**3. Dashboard**
- Query kind 31990 where `#p` = user's pubkey with `operator` marker
- Display list of DVMs with status indicators
- Quick actions: pause/resume, view details

**4. DVM Detail**
- Config editor (relays, blossom, profile)
- Job history table with pagination
- Self-test button with results
- Pause/resume toggle
- Status overview (uptime, jobs, hardware)

### Discovery Flow

```
1. User logs in with Nostr
2. Query relays: {"kinds": [31990], "#p": ["<user_pubkey>"]}
3. Filter: keep only events where p tag has "operator" marker
4. For each DVM:
   a. Send {"cmd": "status"} via encrypted DM
   b. Display in dashboard with live status
```

## Startup Sequences

### First Run (No Identity)

```
1. Generate new keypair
2. Save to identity.key
3. Connect to bootstrap relays
4. Publish kind 0 profile (default name/about)
5. Publish kind 10002 relay list (bootstrap relays)
6. No NIP-78 config found
7. Enter pairing mode
8. Display pairing URL + QR in console
9. Subscribe to DMs, wait for claim_admin
10. On valid claim:
    a. Store admin in NIP-78 config
    b. Publish kind 31990 announcement
    c. Exit pairing mode
    d. Start normal operation
```

### Subsequent Runs (Identity Exists)

```
1. Load keypair from identity.key
2. Connect to bootstrap relays
3. Fetch NIP-78 config (kind 30078, d=video-dvm-config)
4. Decrypt config with own key
5. If admin present:
   a. Connect to configured relays (replace bootstrap)
   b. Subscribe to job requests (kind 5207)
   c. Subscribe to admin DMs (for commands)
   d. Check paused state, start processing if not paused
6. If no admin (config missing/corrupted):
   a. Enter pairing mode
```

### Hot Reload

```
1. Receive encrypted DM from admin pubkey
2. Parse command JSON
3. Validate command and parameters
4. Update in-memory config
5. Publish updated NIP-78 config event
6. Trigger side effects:
   - Relays: disconnect old, connect new, update kind 10002
   - Profile: update kind 0, re-publish kind 31990
   - Blossom: update in-memory, optionally update kind 10063
   - Pause/resume: update job processing state
7. Respond to admin with result
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `BOOTSTRAP_RELAYS` | No | `wss://relay.damus.io,wss://nos.lol,wss://relay.nostr.band` | Initial relays before config loaded |
| `DVM_ADMIN_APP_URL` | No | `https://dvm-admin.example.com` | Base URL for pairing links |
| `DATA_DIR` | No | `~/.local/share/dvm-video` | Directory for identity.key |
| `HTTP_PORT` | No | `3000` | Port for bundled web app |
| `RUST_LOG` | No | `info` | Logging level |

## Security Considerations

### Identity Key

- File permissions: 600 (owner read/write only)
- Never transmitted over network
- Backup responsibility on operator

### Pairing

- Secret visible only via console (physical/SSH access)
- One-time use, short expiration
- No remote pairing initiation

### Admin Commands

- NIP-44 encryption (authenticated encryption)
- Only accepted from configured admin pubkey
- Commands logged for audit

### Config Storage

- Encrypted to DVM's own pubkey
- Only DVM can decrypt
- Relays see encrypted blob only

## Migration Path

For existing deployments using environment variables:

1. Run DVM with existing env vars (continues to work)
2. Go through pairing flow to set admin
3. Admin sends config commands to migrate settings to Nostr
4. Remove env vars (except bootstrap relays)
5. DVM now fully remote-configured

Alternatively, a migration command could import existing env config:
```json
{"cmd": "import_env_config"}
```

## Future Considerations

- **Multi-admin support:** Multiple operator pubkeys with different permission levels
- **Config history:** Keep previous config versions for rollback
- **Webhooks:** Notify external systems on job completion
- **Metrics export:** Prometheus-compatible metrics endpoint
- **Fleet management:** Manage multiple DVMs with shared config templates
