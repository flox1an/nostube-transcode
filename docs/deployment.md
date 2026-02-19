# Deployment Guide

Detailed setup instructions for running the Video Transform DVM on different platforms.

## Prerequisites

- Docker and Docker Compose (for containerized deployment)
- An NVIDIA, Intel, or AMD GPU for hardware-accelerated encoding (optional -- falls back to software)
- A Nostr keypair for the operator (`OPERATOR_NPUB`)

## Docker Deployment

### NVIDIA GPU

#### 1. Install NVIDIA Driver

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install nvidia-driver-550  # or latest available
sudo reboot
```

**Fedora:**
```bash
sudo dnf install akmod-nvidia
sudo reboot
```

Verify:
```bash
nvidia-smi
```

#### 2. Install NVIDIA Container Toolkit

```bash
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | \
  sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg

curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
  sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
  sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list

sudo apt update && sudo apt install nvidia-container-toolkit
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

For Fedora, see the [NVIDIA Container Toolkit install guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html).

#### 3. Run

```bash
cp .env.example .env
# Edit .env and set OPERATOR_NPUB to your npub
docker compose -f docker-compose.nvidia.yml up -d
```

### Intel GPU (QSV/VAAPI)

The default `docker-compose.yml` is configured for Intel GPUs.

The container needs `/dev/dri` access. You may need to adjust the `group_add` GIDs in `docker-compose.yml` to match your host:

```bash
# Find your host's video/render group IDs
getent group video render
stat -c '%g' /dev/dri/renderD128
```

Then update the GIDs in `docker-compose.yml` and run:

```bash
cp .env.example .env
# Edit .env and set OPERATOR_NPUB
docker compose up -d
```

### CPU Only (No GPU)

Use the default compose file without GPU devices. The DVM auto-detects hardware and falls back to software encoding (libx264/libx265).

```bash
cp .env.example .env
# Edit .env and set OPERATOR_NPUB
docker compose up -d
```

## Building from Source

### Dependencies

- Rust 1.85+
- Node.js 20+ (for the frontend)
- FFmpeg with hardware acceleration support

### Install FFmpeg

**Ubuntu/Debian:**
```bash
sudo apt install ffmpeg
```

**Fedora (RPM Fusion):**
```bash
sudo dnf install \
  https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm \
  https://mirrors.rpmfusion.org/nonfree/fedora/rpmfusion-nonfree-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install ffmpeg
```

For NVIDIA NVENC support, your system FFmpeg must be built with `--enable-nvenc`. Most distro packages include this when CUDA libraries are present.

Verify encoder availability:
```bash
ffmpeg -encoders 2>/dev/null | grep -E "nvenc|vaapi|qsv"
```

### Build and Run

```bash
# Build frontend
cd frontend && npm ci && npm run build && cd ..

# Build Rust binary
cargo build --release

# Run
OPERATOR_NPUB=npub1... ./target/release/nostube-transcode
```

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `OPERATOR_NPUB` | Yes | -- | Nostr pubkey (npub or hex) of the operator/admin |
| `BOOTSTRAP_RELAYS` | No | `wss://relay.damus.io,wss://nos.lol` | Comma-separated bootstrap relays |
| `HTTP_PORT` | No | `3000` | Port for the admin web UI |
| `TEMP_DIR` | No | `./temp` | Directory for temporary video files |
| `RUST_LOG` | No | `info` | Log level (`debug`, `info`, `warn`, `error`) |
| `FFMPEG_PATH` | No | System PATH | Path to ffmpeg binary |
| `FFPROBE_PATH` | No | System PATH | Path to ffprobe binary |

## Verifying Hardware Acceleration

Check the DVM logs on startup:

```bash
# Docker
docker logs nostube-transcode

# Native
RUST_LOG=info ./target/release/nostube-transcode
```

Look for:
```
Detected NVIDIA GPU, using NVENC hardware acceleration
```
or:
```
Detected VAAPI hardware acceleration
```
or:
```
No hardware acceleration detected, using software encoding
```

## Troubleshooting

### NVIDIA: "Failed to initialize NVML"
The NVIDIA Container Toolkit is not installed or Docker is not configured. Re-run the toolkit setup and restart Docker.

### Intel: "VAAPI probe failed"
The Intel media driver may not be installed inside the container, or the `/dev/dri` device is not passed through. Check that `devices: - /dev/dri:/dev/dri` is in your compose file and the group IDs match.

### "OPERATOR_NPUB environment variable is required"
You must set `OPERATOR_NPUB` in your `.env` file before starting the DVM.

### Slow encoding (software fallback)
If logs show "Software" instead of a hardware encoder, the GPU is not being detected. For Docker, ensure the correct compose file is used and GPU devices are passed through.
