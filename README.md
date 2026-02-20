# Video Transform DVM

A Nostr [Data Vending Machine](https://www.data-vending-machines.org/) that transforms videos into HLS format and uploads them to Blossom servers. Hardware-accelerated encoding with NVIDIA NVENC, Intel QSV/VAAPI, and Apple VideoToolbox.

## Install

Download a static binary (Linux x86_64 or macOS Apple Silicon):

```bash
curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
```

Pin a specific version:

```bash
VERSION=v0.1.2 curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
```

## Quick Start (Docker)

**One-liner** (detects GPU, prompts for config, starts everything):

```bash
git clone https://github.com/flox1an/nostube-transcode.git && cd nostube-transcode && ./setup.sh
```

**Manual Docker setup:**

```bash
git clone https://github.com/flox1an/nostube-transcode.git
cd nostube-transcode
cp .env.example .env
# Edit .env -- set OPERATOR_NPUB to your npub
```

NVIDIA GPU: `docker compose -f docker-compose.nvidia.yml up -d`
Intel GPU / CPU: `docker compose up -d`

Open `http://localhost:3000` to manage your DVM.

> NVIDIA users need the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) installed on the host. See [docs/deployment.md](docs/deployment.md) for full setup instructions.

## Configuration

The DVM requires one environment variable:

| Variable | Description |
|---|---|
| `OPERATOR_NPUB` | **(Required)** Your Nostr pubkey (npub or hex). The DVM only accepts admin commands from this key. |

All other configuration (relays, Blossom servers, profile) is managed remotely via the admin UI or admin commands over Nostr. Config is stored encrypted on Nostr relays using [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md).

See [docs/deployment.md](docs/deployment.md) for the full list of optional environment variables.

### File Locations

The DVM follows the [XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/) convention on all platforms:

| Path | Purpose | Override |
|---|---|---|
| `~/.local/share/nostube-transcode/identity.key` | DVM keypair (auto-generated) | `DATA_DIR` |
| `~/.local/share/nostube-transcode/env` | Environment config (created by installer) | - |
| `~/.cache/nostube-transcode/` | Temporary ffmpeg working files (auto-cleaned) | `TEMP_DIR` |

Docker deployments use `TEMP_DIR=/app/temp` and can mount the identity key via Docker secrets.

## Features

- Multi-resolution adaptive HLS (240p through 4K)
- H.264 and H.265 codec support
- AES-128 HLS encryption
- Hardware-accelerated encoding (NVIDIA, Intel, Apple, or software fallback)
- Embedded admin web UI
- Remote configuration via Nostr (NIP-78)
- Encrypted admin commands via Nostr (NIP-44)

## Documentation

- [Deployment Guide](docs/deployment.md) -- Docker setup, GPU drivers, building from source, environment variables, troubleshooting
- [Admin Protocol](docs/admin-protocol.md) -- Encrypted RPC protocol for remote DVM management

## Development

```bash
# Build and run
cd frontend && npm ci && npm run build && cd ..
OPERATOR_NPUB=npub1... cargo run

# Debug logging
RUST_LOG=debug cargo run

# Tests and linting
cargo test
cargo clippy
cargo fmt --check
```

## License

MIT
