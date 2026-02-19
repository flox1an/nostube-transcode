# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Nostr Data Vending Machine (DVM) that transforms videos into HLS format and uploads them to Blossom servers. The DVM listens for video transformation requests on Nostr relays, processes videos using FFmpeg, and publishes results back to the network.

## Build & Run Commands

```bash
# Build and run
cargo run

# Build release
cargo build --release

# Run tests
cargo test

# Run specific test
cargo test test_ffmpeg_command

# Check/lint
cargo check
cargo clippy
cargo fmt --check

# With debug logging
RUST_LOG=nostube_transcode=debug cargo run
```

### Frontend (React/Vite in `frontend/`)

```bash
cd frontend
npm install
npm run dev      # Development server
npm run build    # Production build (output embedded in Rust binary)
npm run lint
```

## Architecture

### Core Components

- **main.rs** - Entry point, initializes DVM in remote config mode
- **config.rs** - Configuration from remote Nostr config (NIP-78)
- **lib.rs** - Re-exports all modules for testing

### Module Structure

- **dvm/** - Nostr DVM protocol handling
  - `handler.rs` - Job processing, orchestrates video transformation and upload
  - `events.rs` - DVM event kinds (5207 request, 6207 result, 7000 status), job context parsing
  - `encryption.rs` - NIP-04 encryption support

- **nostr/** - Nostr network layer
  - `client.rs` - Subscription manager, relay connections, event deduplication
  - `publisher.rs` - Event publishing with retry logic

- **video/** - FFmpeg video processing
  - `transform.rs` - Main `VideoProcessor` struct, HLS transformation pipeline, output collection
  - `ffmpeg.rs` - FFmpeg command building for multi-resolution HLS output
  - `playlist.rs` - M3U8 parsing and rewriting (segment URLs to SHA-256 hashes)
  - `metadata.rs` - ffprobe metadata extraction

- **blossom/** - Blossom server integration (file storage)
  - `client.rs` - Upload with streaming, `BlobDescriptor` type
  - `auth.rs` - Kind 24242 auth token generation
  - `cleanup.rs` - Blob expiration scheduler

- **web/** - Embedded HTTP server (Axum)
  - `mod.rs` - SPA-style routing, serves embedded frontend
  - `assets.rs` - rust-embed integration for static files

- **util/** - Helpers
  - `hash.rs` - SHA-256 streaming file hasher
  - `temp.rs` - Temp directory management with cleanup

### Data Flow

1. `SubscriptionManager` receives kind 5207 events from Nostr relays
2. Events are parsed into `JobContext` and sent via mpsc channel
3. `JobHandler` receives jobs, validates input, sends status updates
4. `VideoProcessor` downloads and transforms video to HLS using FFmpeg
5. `BlossomClient` uploads segments/playlists, rewrites URLs to SHA-256 hashes
6. Result event (kind 6207) published with master playlist URL

## Remote Configuration

The DVM startup flow:

1. Generate an identity (saved to `~/.local/share/nostube-transcode/identity.key`)
2. Read `OPERATOR_NPUB` env var (required, panics if missing/invalid)
3. Connect to bootstrap relays
4. Fetch remote config from Nostr (NIP-78) if it exists
5. Set admin from `OPERATOR_NPUB` if not already in remote config
6. Start normal operation

### Remote Config Modules
- `src/identity.rs` - Identity key persistence
- `src/bootstrap.rs` - Bootstrap relay management
- `src/remote_config.rs` - NIP-78 config storage
- `src/admin/` - Admin command handling
- `src/dvm_state.rs` - Runtime state management
- `src/startup.rs` - Startup orchestration

### Admin Commands
Sent as NIP-04 encrypted DMs:
- `get_config` - Get current configuration
- `set_relays` - Update relay list
- `set_blossom_servers` - Update Blossom servers
- `set_profile` - Update name/about
- `pause` / `resume` - Control job processing
- `status` - Get DVM status
- `job_history` - Get recent jobs

## Environment Variables

Required:
- `OPERATOR_NPUB` - Nostr pubkey (npub or hex) of the DVM operator/admin. The DVM refuses to start without it.

Optional (runtime configuration is managed via remote config NIP-78):
- `BOOTSTRAP_RELAYS` - Comma-separated bootstrap relays (default: wss://relay.damus.io,wss://nos.lol,wss://relay.primal.net)
- `HTTP_PORT` - Default 3000
- `TEMP_DIR` - Default ./temp
- `FFMPEG_PATH` / `FFPROBE_PATH` - Default uses system PATH
- `RUST_LOG` - Logging level

## Key Dependencies

- `nostr-sdk` 0.35 - Nostr protocol
- `tokio` - Async runtime
- `axum` - HTTP server
- `reqwest` - HTTP client
- `rust-embed` - Static file embedding

## FFmpeg Notes

- Uses H.265 (libx265 on Linux, hevc_videotoolbox on macOS for hardware acceleration)
- Default output: 360p, 720p, 1080p (original) with fMP4 segments
- 6-second segment duration for HLS
