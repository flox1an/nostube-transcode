# Video Transform DVM

A Nostr [Data Vending Machine](https://www.data-vending-machines.org/) that transforms videos into HLS format and uploads them to Blossom servers. Hardware-accelerated encoding with NVIDIA NVENC, Intel QSV/VAAPI, and Apple VideoToolbox.

## Quick Start

There are two ways to run the DVM: **Docker** (recommended) or **standalone binary**.

### Option A: Docker

Best for production deployments. Pre-built images are available for Intel/AMD and NVIDIA GPUs.

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

Then start with the compose file matching your hardware:

| Hardware | Command |
|---|---|
| **NVIDIA GPU** | `docker compose -f docker-compose.nvidia.yml up -d` |
| **Intel GPU** | `docker compose up -d` |
| **CPU only** | `docker compose up -d` |

Open `http://localhost:5207` to manage your DVM.

#### NVIDIA GPU Requirements (Docker)

1. NVIDIA driver >= 525 on the host
2. [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) installed and configured for Docker
3. Verify with: `nvidia-smi` (should show your GPU)

See [docs/deployment.md](docs/deployment.md) for step-by-step NVIDIA driver and toolkit setup.

#### Intel GPU Requirements (Docker)

The default `docker-compose.yml` passes `/dev/dri` into the container. You may need to adjust the `group_add` GIDs to match your host:

```bash
getent group video render
stat -c '%g' /dev/dri/renderD128
```

### Option B: Standalone Binary

Best for development or systems without Docker. The DVM runs as a single binary with no external dependencies besides FFmpeg.

**Install via script:**

```bash
curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
```

Pin a specific version:

```bash
VERSION=v0.1.2 curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
```

**Or build from source:**

```bash
cd frontend && npm ci && npm run build && cd ..
OPERATOR_NPUB=npub1... cargo run --release
```

**Standalone prerequisites:**

- FFmpeg installed with hardware acceleration support for your GPU
- Verify encoder availability: `ffmpeg -encoders 2>/dev/null | grep -E "nvenc|vaapi|qsv"`
- For NVIDIA: CUDA toolkit and `libnvidia-encode` must be installed on the host

## Configuration

The DVM requires one environment variable:

| Variable | Description |
|---|---|
| `OPERATOR_NPUB` | **(Required)** Your Nostr pubkey (npub or hex). The DVM only accepts admin commands from this key. |

All other configuration (relays, Blossom servers, profile, concurrency) is managed remotely via the admin UI or admin commands over Nostr. Config is stored encrypted on Nostr relays using [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md).

See [docs/deployment.md](docs/deployment.md) for the full list of optional environment variables.

### File Locations

| Path | Purpose | Override |
|---|---|---|
| `~/.local/share/nostube-transcode/identity.key` | DVM keypair (auto-generated) | `DATA_DIR` |
| `~/.local/share/nostube-transcode/env` | Environment config (created by installer) | - |
| `~/.cache/nostube-transcode/` | Temporary ffmpeg working files (auto-cleaned) | `TEMP_DIR` |

Docker deployments use `TEMP_DIR=/app/temp` and persist the identity key via a volume mount.

## Hardware Acceleration

The DVM auto-detects available GPU hardware at startup and selects the best encoder. Check the logs for which encoder was selected:

```
Detected NVIDIA GPU, using NVENC hardware acceleration
Detected VAAPI hardware acceleration
No hardware acceleration detected, using software encoding
```

You can also check via the admin UI at `http://localhost:5207` or the `system_info` admin command.

| GPU | Encoder | H.264 | H.265 | AV1 |
|---|---|---|---|---|
| NVIDIA GeForce/Quadro | NVENC | Yes | Yes | RTX 40xx+ only |
| Intel (6th gen+) | VAAPI/QSV | Yes | Yes | 12th gen+ |
| Apple Silicon | VideoToolbox | Yes | Yes | M3+ |
| CPU fallback | libx264/x265 | Yes | Yes | Yes (slow) |

### Concurrent Jobs

By default the DVM processes one video at a time. With a powerful GPU you can increase this via the admin UI or the `set_config` command:

```json
{"id":"1","method":"set_config","params":{"max_concurrent_jobs": 3}}
```

Note: NVIDIA GeForce cards have an NVENC session limit (max 5 on newer, 3 on older cards). Keep `max_concurrent_jobs` within this limit.

## Running as a Daemon (Standalone)

The installer generates a daemon config for your platform. By default these are **user-level services** that run while you are logged in. See the sections below to promote them to system services that survive logout.

### Linux (systemd)

```bash
# Start as a user service (runs while logged in)
systemctl --user enable --now nostube-transcode

# Check status / logs
systemctl --user status nostube-transcode
journalctl --user -u nostube-transcode -f

# Stop and disable
systemctl --user disable --now nostube-transcode
```

To keep the service running after logout (system service):

```bash
loginctl enable-linger $USER
```

This tells systemd to start your user services at boot and keep them running regardless of login sessions.

### Linux (SysV init)

For distributions without systemd (e.g., MX Linux, Devuan), the installer generates a SysV init script at `~/.local/share/nostube-transcode/nostube-transcode.initd`. Install it as a system service when ready:

```bash
sudo cp ~/.local/share/nostube-transcode/nostube-transcode.initd /etc/init.d/nostube-transcode
sudo update-rc.d nostube-transcode defaults
sudo service nostube-transcode start
```

```bash
# Check status / stop
sudo service nostube-transcode status
sudo service nostube-transcode stop

# Remove from boot
sudo update-rc.d nostube-transcode remove
```

### macOS (launchd)

```bash
# Start as a user agent (runs while logged in)
launchctl load ~/Library/LaunchAgents/com.nostube.transcode.plist

# Check logs
tail -f ~/.local/share/nostube-transcode/stderr.log

# Stop
launchctl unload ~/Library/LaunchAgents/com.nostube.transcode.plist
```

To run as a system daemon that survives logout, copy the plist to LaunchDaemons:

```bash
sudo cp ~/Library/LaunchAgents/com.nostube.transcode.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/com.nostube.transcode.plist
```

## Features

- Multi-resolution adaptive HLS (240p through 4K)
- H.264, H.265 and AV1 codec support
- AES-128 HLS encryption
- Hardware-accelerated encoding (NVIDIA, Intel, Apple, or software fallback)
- Configurable concurrent job processing
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
