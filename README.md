# Video Transform DVM

A Nostr [Data Vending Machine](https://www.data-vending-machines.org/) that transforms videos into HLS format and uploads them to Blossom servers. Hardware-accelerated encoding with NVIDIA NVENC, Intel QSV/VAAPI, and Apple VideoToolbox.

## Quick Start (Docker)

```bash
git clone https://github.com/nickhntv/divico-dvm.git
cd divico-dvm
cp .env.example .env
# Edit .env -- set OPERATOR_NPUB to your npub
```

**NVIDIA GPU:**
```bash
docker compose -f docker-compose.nvidia.yml up -d
```

**Intel GPU / CPU:**
```bash
docker compose up -d
```

Open `http://localhost:3000` to manage your DVM.

> NVIDIA users need the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) installed on the host. See [docs/deployment.md](docs/deployment.md) for full setup instructions.

## Configuration

The DVM requires one environment variable:

| Variable | Description |
|---|---|
| `OPERATOR_NPUB` | **(Required)** Your Nostr pubkey (npub or hex). The DVM only accepts admin commands from this key. |

All other configuration (relays, Blossom servers, profile) is managed remotely via the admin UI or admin commands over Nostr. Config is stored encrypted on Nostr relays using [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md).

See [docs/deployment.md](docs/deployment.md) for the full list of optional environment variables.

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
