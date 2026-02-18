#!/usr/bin/env bash
set -euo pipefail

# Video Transform DVM - Setup Script
# Detects GPU, checks prerequisites, generates .env, and starts the container.

REPO_URL="https://github.com/nickhntv/divico-dvm.git"
COMPOSE_NVIDIA="docker-compose.nvidia.yml"
COMPOSE_DEFAULT="docker-compose.yml"

# Colors (if terminal supports them)
if [ -t 1 ]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  YELLOW='\033[1;33m'
  BOLD='\033[1m'
  NC='\033[0m'
else
  RED='' GREEN='' YELLOW='' BOLD='' NC=''
fi

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[x]${NC} $*"; }
bold()  { echo -e "${BOLD}$*${NC}"; }

# --- Checks ---

check_docker() {
  if ! command -v docker &>/dev/null; then
    error "Docker is not installed."
    echo "  Install it from https://docs.docker.com/engine/install/"
    exit 1
  fi

  if ! docker info &>/dev/null; then
    error "Docker daemon is not running, or your user lacks permissions."
    echo "  Try: sudo systemctl start docker"
    echo "  Or add your user to the docker group: sudo usermod -aG docker \$USER"
    exit 1
  fi

  if ! docker compose version &>/dev/null; then
    error "Docker Compose v2 is not available."
    echo "  Install it from https://docs.docker.com/compose/install/"
    exit 1
  fi

  info "Docker and Docker Compose are available."
}

detect_gpu() {
  GPU_TYPE="cpu"
  COMPOSE_FILE="$COMPOSE_DEFAULT"

  # Check for NVIDIA GPU
  if [ -e /dev/nvidia0 ] || command -v nvidia-smi &>/dev/null; then
    if nvidia-smi &>/dev/null; then
      GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader,nounits 2>/dev/null | head -1)
      info "Detected NVIDIA GPU: ${GPU_NAME:-unknown}"
      GPU_TYPE="nvidia"
      COMPOSE_FILE="$COMPOSE_NVIDIA"
    else
      warn "NVIDIA device found but nvidia-smi failed. Driver may not be installed."
    fi
  fi

  # Check for Intel GPU (if no NVIDIA)
  if [ "$GPU_TYPE" = "cpu" ] && [ -e /dev/dri/renderD128 ]; then
    if command -v vainfo &>/dev/null && vainfo &>/dev/null 2>&1; then
      info "Detected Intel/AMD GPU with VAAPI support."
      GPU_TYPE="vaapi"
      COMPOSE_FILE="$COMPOSE_DEFAULT"
    else
      info "Render device found at /dev/dri but VAAPI not verified."
      GPU_TYPE="vaapi"
      COMPOSE_FILE="$COMPOSE_DEFAULT"
    fi
  fi

  if [ "$GPU_TYPE" = "cpu" ]; then
    warn "No GPU detected. Will use software encoding (slower but works)."
  fi
}

check_nvidia_container_toolkit() {
  if [ "$GPU_TYPE" != "nvidia" ]; then
    return
  fi

  if ! docker run --rm --gpus all nvidia/cuda:12.6.3-base-ubuntu24.04 nvidia-smi &>/dev/null 2>&1; then
    error "NVIDIA Container Toolkit is not working."
    echo ""
    echo "  Install it with:"
    echo ""
    echo "    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | \\"
    echo "      sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg"
    echo ""
    echo "    curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \\"
    echo "      sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \\"
    echo "      sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list"
    echo ""
    echo "    sudo apt update && sudo apt install nvidia-container-toolkit"
    echo "    sudo nvidia-ctk runtime configure --runtime=docker"
    echo "    sudo systemctl restart docker"
    echo ""
    echo "  For other distros, see: https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html"
    echo ""
    read -rp "Continue without GPU acceleration? [y/N] " yn
    case "$yn" in
      [yY]*) GPU_TYPE="cpu"; COMPOSE_FILE="$COMPOSE_DEFAULT" ;;
      *) exit 1 ;;
    esac
  else
    info "NVIDIA Container Toolkit is working."
  fi
}

prompt_operator_npub() {
  if [ -f .env ] && grep -q '^OPERATOR_NPUB=' .env 2>/dev/null; then
    EXISTING=$(grep '^OPERATOR_NPUB=' .env | cut -d= -f2-)
    if [ -n "$EXISTING" ] && [ "$EXISTING" != "npub1..." ]; then
      info "Found existing OPERATOR_NPUB in .env"
      read -rp "Keep current value? [Y/n] " yn
      case "$yn" in
        [nN]*) ;;
        *) return ;;
      esac
    fi
  fi

  echo ""
  bold "Your Nostr pubkey (npub) is required."
  echo "The DVM will only accept admin commands from this key."
  echo ""

  while true; do
    read -rp "Enter your OPERATOR_NPUB (npub1... or hex): " OPERATOR_NPUB
    if [ -z "$OPERATOR_NPUB" ]; then
      error "OPERATOR_NPUB is required. The DVM cannot start without it."
      continue
    fi
    if [[ "$OPERATOR_NPUB" == npub1* ]] || [[ "$OPERATOR_NPUB" =~ ^[0-9a-fA-F]{64}$ ]]; then
      break
    fi
    error "Invalid format. Must be an npub (npub1...) or 64-char hex pubkey."
  done
}

write_env() {
  if [ -f .env ]; then
    # Update existing .env
    if grep -q '^OPERATOR_NPUB=' .env; then
      sed -i.bak "s|^OPERATOR_NPUB=.*|OPERATOR_NPUB=${OPERATOR_NPUB}|" .env
      rm -f .env.bak
    else
      echo "OPERATOR_NPUB=${OPERATOR_NPUB}" >> .env
    fi
  else
    # Create from example or from scratch
    if [ -f .env.example ]; then
      cp .env.example .env
      sed -i.bak "s|^OPERATOR_NPUB=.*|OPERATOR_NPUB=${OPERATOR_NPUB}|" .env
      rm -f .env.bak
    else
      echo "OPERATOR_NPUB=${OPERATOR_NPUB}" > .env
    fi
  fi

  info "Wrote OPERATOR_NPUB to .env"
}

clone_if_needed() {
  # If we're already in the repo directory, skip cloning
  if [ -f docker-compose.yml ] && [ -f Dockerfile.nvidia ]; then
    return
  fi

  # If run via curl|bash, we need to clone first
  info "Cloning repository..."
  INSTALL_DIR="divico-dvm"
  if [ -d "$INSTALL_DIR" ]; then
    info "Directory $INSTALL_DIR already exists, using it."
  else
    git clone "$REPO_URL" "$INSTALL_DIR"
  fi
  cd "$INSTALL_DIR"
}

start_dvm() {
  echo ""
  bold "Ready to start the DVM."
  echo "  GPU:          ${GPU_TYPE}"
  echo "  Compose file: ${COMPOSE_FILE}"
  echo "  Admin UI:     http://localhost:3000"
  echo ""

  read -rp "Start now? [Y/n] " yn
  case "$yn" in
    [nN]*)
      info "Skipped. To start later, run:"
      echo "  docker compose -f ${COMPOSE_FILE} up -d"
      return
      ;;
  esac

  info "Building and starting container..."
  docker compose -f "$COMPOSE_FILE" up -d --build

  echo ""
  info "DVM is starting. Open http://localhost:3000 to manage it."
  echo ""
  echo "  View logs:  docker logs -f dvm-video-processing"
  echo "  Stop:       docker compose -f ${COMPOSE_FILE} down"
  echo ""
}

# --- Main ---

main() {
  echo ""
  bold "Video Transform DVM - Setup"
  echo ""

  check_docker
  clone_if_needed
  detect_gpu
  check_nvidia_container_toolkit
  prompt_operator_npub
  write_env
  start_dvm
}

main "$@"
