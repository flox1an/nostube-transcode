# Video Transform DVM

A Nostr Data Vending Machine (DVM) that transforms videos into HLS format and uploads them to Blossom servers. The DVM listens for video transformation requests on Nostr relays, processes videos using FFmpeg with hardware acceleration, and publishes results back to the network.

## Features

- Multi-resolution HLS output (360p, 720p, 1080p)
- Hardware-accelerated encoding (NVIDIA NVENC, Intel QSV/VAAPI, Apple VideoToolbox)
- H.264 and H.265 codec support
- AES-128 HLS encryption
- Automatic hardware detection and fallback to software encoding
- Embedded web UI for job submission

## Quick Start

### Using Docker (Recommended)

**NVIDIA GPU:**
```bash
docker compose -f docker-compose.nvidia.yml up -d
```

**Intel GPU (QSV/VAAPI):**
```bash
docker compose up -d
```

### From Source

```bash
# Install dependencies (see Linux Installation below)
cargo build --release
./target/release/dvm-video-processing
```

## Configuration

The DVM uses **zero-config startup** with remote configuration via Nostr:

1. **First Run**: The DVM generates an identity and enters pairing mode
2. **Pairing**: Scan the QR code or click the pairing link to configure via the admin web app
3. **Remote Config**: All configuration is stored on Nostr (NIP-78) and synced automatically

### Optional Environment Variables

```bash
# Optional: Bootstrap relays for initial connection (default: wss://relay.damus.io,wss://nos.lol)
BOOTSTRAP_RELAYS=wss://relay.damus.io,wss://nos.lol

# Optional: Admin web app URL for pairing
DVM_ADMIN_APP_URL=https://admin.example.com

# Optional: HTTP port for web UI (default: 3000)
HTTP_PORT=3000

# Optional: Temp directory (default: ./temp)
TEMP_DIR=./temp

# Optional: Logging level
RUST_LOG=info
```

### Admin Commands

Once paired, send encrypted DMs to manage your DVM:
- `get_config` - View current configuration
- `set_relays <relay1,relay2>` - Update relay list
- `set_blossom_servers <server1,server2>` - Update Blossom servers
- `set_profile <name> <about>` - Update DVM name and description
- `pause` / `resume` - Control job processing
- `status` - Get DVM status
- `job_history` - View recent jobs

## Linux Installation

### Prerequisites

- FFmpeg with hardware acceleration support
- Rust 1.75+ (for building from source)
- Node.js 20+ (for building frontend)

### NVIDIA GPU Setup

#### 1. Install NVIDIA Driver

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install nvidia-driver-550  # or latest version
sudo reboot
```

**Fedora:**
```bash
sudo dnf install akmod-nvidia
sudo reboot
```

Verify installation:
```bash
nvidia-smi
```

#### 2. Install FFmpeg with NVENC

**Ubuntu 22.04+ (via PPA):**
```bash
sudo add-apt-repository ppa:ubuntuhandbook1/ffmpeg7
sudo apt update
sudo apt install ffmpeg
```

**Fedora (via RPM Fusion):**
```bash
sudo dnf install \
  https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm \
  https://mirrors.rpmfusion.org/nonfree/fedora/rpmfusion-nonfree-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install ffmpeg
```

**From Source (any distro):**
```bash
# Install NVIDIA CUDA toolkit first
sudo apt install nvidia-cuda-toolkit  # Ubuntu/Debian
# or
sudo dnf install cuda                  # Fedora

# Clone and build FFmpeg
git clone https://git.ffmpeg.org/ffmpeg.git
cd ffmpeg
./configure \
  --enable-gpl \
  --enable-nonfree \
  --enable-cuda-nvcc \
  --enable-libnpp \
  --enable-nvenc \
  --enable-nvdec \
  --enable-libx264 \
  --enable-libx265
make -j$(nproc)
sudo make install
```

Verify NVENC support:
```bash
ffmpeg -encoders 2>/dev/null | grep nvenc
# Should show: hevc_nvenc, h264_nvenc
```

#### 3. Install NVIDIA Container Toolkit (for Docker)

```bash
# Add NVIDIA container toolkit repository
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | \
  sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg

curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
  sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
  sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list

sudo apt update
sudo apt install nvidia-container-toolkit

# Configure Docker runtime
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

### Intel GPU Setup (QSV/VAAPI)

#### 1. Install Intel Media Drivers

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install \
  intel-media-va-driver-non-free \
  libva-drm2 \
  libva2 \
  vainfo
```

**Fedora:**
```bash
sudo dnf install \
  intel-media-driver \
  libva \
  libva-utils
```

#### 2. Verify VAAPI

```bash
vainfo
# Should show supported profiles for your GPU
```

#### 3. Install FFmpeg

Most distro FFmpeg packages include VAAPI support:
```bash
sudo apt install ffmpeg   # Ubuntu/Debian
sudo dnf install ffmpeg   # Fedora (RPM Fusion)
```

### AMD GPU Setup (VAAPI)

#### 1. Install Mesa VA-API Drivers

**Ubuntu/Debian:**
```bash
sudo apt install mesa-va-drivers vainfo
```

**Fedora:**
```bash
sudo dnf install mesa-va-drivers libva-utils
```

#### 2. Verify and Install FFmpeg

```bash
vainfo
sudo apt install ffmpeg  # or dnf install ffmpeg
```

### Software-Only (No GPU)

If no GPU is available, the DVM will automatically fall back to software encoding using libx264/libx265:

```bash
sudo apt install ffmpeg  # Ubuntu/Debian
sudo dnf install ffmpeg  # Fedora (RPM Fusion)
```

### Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone https://github.com/your-repo/rust-video-dvm.git
cd rust-video-dvm

# Build frontend
cd frontend
npm install
npm run build
cd ..

# Build Rust binary
cargo build --release

# Run
./target/release/dvm-video-processing
```

## Docker Deployment

### NVIDIA GPU

Use `docker-compose.nvidia.yml`:

```bash
docker compose -f docker-compose.nvidia.yml up -d
```

Requirements:
- NVIDIA driver installed on host
- NVIDIA Container Toolkit installed
- Docker configured with nvidia runtime

### Intel GPU (QSV/VAAPI)

Use the default `docker-compose.yml`:

```bash
docker compose up -d
```

The container needs access to `/dev/dri` for GPU acceleration. Adjust group IDs in `docker-compose.yml` to match your host:

```bash
# Find your host's video/render group IDs
getent group video render
stat /dev/dri/renderD128
```

### CPU Only

Build with the standard Dockerfile but without GPU device mapping:

```bash
docker build -t dvm-video .
docker run -d \
  --name dvm-video \
  -p 3000:3000 \
  --env-file .env \
  dvm-video
```

## Verifying Hardware Acceleration

Check the logs when a job runs:

```bash
# Docker
docker logs dvm-video-processing

# Native
RUST_LOG=debug ./target/release/dvm-video-processing
```

Look for lines like:
- `Detected NVIDIA GPU, using NVENC hardware acceleration`
- `Detected VAAPI hardware acceleration`
- `Detected Intel QSV hardware acceleration`
- `No hardware acceleration detected, using software encoding`

## Development

```bash
# Run in development mode
cargo run

# Run with debug logging
RUST_LOG=dvm_video_processing=debug cargo run

# Run tests
cargo test

# Check/lint
cargo check
cargo clippy
cargo fmt --check
```

## Architecture

```
src/
├── main.rs              # Entry point, async task spawning
├── config.rs            # Environment configuration
├── dvm/                 # Nostr DVM protocol
│   ├── handler.rs       # Job processing orchestration
│   ├── events.rs        # DVM event parsing (5207/6207/7000)
│   └── encryption.rs    # NIP-04 encryption
├── nostr/               # Nostr network layer
│   ├── client.rs        # Subscription management
│   └── publisher.rs     # Event publishing
├── video/               # FFmpeg video processing
│   ├── transform.rs     # HLS transformation pipeline
│   ├── ffmpeg.rs        # FFmpeg command building
│   ├── hwaccel.rs       # Hardware acceleration detection
│   ├── playlist.rs      # M3U8 parsing/rewriting
│   └── metadata.rs      # ffprobe metadata extraction
├── blossom/             # Blossom file storage
│   ├── client.rs        # Upload with streaming
│   ├── auth.rs          # Kind 24242 auth tokens
│   └── cleanup.rs       # Blob expiration
└── web/                 # Embedded HTTP server
    └── mod.rs           # Axum routes, embedded frontend
```

## License

MIT
