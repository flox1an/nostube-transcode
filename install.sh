#!/usr/bin/env bash
set -euo pipefail

# nostube-transcode installer
# Usage: curl -sSf https://raw.githubusercontent.com/flox1an/nostube-transcode/main/install.sh | bash
# Pin version: VERSION=v0.2.0 curl -sSf ... | bash

REPO="flox1an/nostube-transcode"
BINARY_NAME="nostube-transcode"
INSTALL_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.local/share/nostube-transcode"
ENV_FILE="${DATA_DIR}/env"

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

# --- Detect platform ---

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "${os}" in
    linux)
      case "${arch}" in
        x86_64)  PLATFORM="x86_64-linux" ;;
        *)       error "Unsupported Linux architecture: ${arch} (only x86_64 is supported)"; exit 1 ;;
      esac
      ;;
    darwin)
      case "${arch}" in
        arm64)   PLATFORM="aarch64-darwin" ;;
        *)       error "Unsupported macOS architecture: ${arch} (only Apple Silicon is supported)"; exit 1 ;;
      esac
      ;;
    *)
      error "Unsupported OS: ${os}"
      exit 1
      ;;
  esac

  info "Detected platform: ${PLATFORM}"
}

# --- Determine version ---

determine_version() {
  if [ -n "${VERSION:-}" ]; then
    TAG="$VERSION"
    info "Using pinned version: ${TAG}"
    return
  fi

  info "Fetching latest release..."
  TAG=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)

  if [ -z "$TAG" ]; then
    error "Failed to determine latest release. Set VERSION env var to pin a version."
    exit 1
  fi

  info "Latest version: ${TAG}"
}

# --- Check existing install ---

check_existing() {
  if [ -x "${INSTALL_DIR}/${BINARY_NAME}" ]; then
    warn "Existing installation found. Upgrading to ${TAG}..."
  fi
}

# --- Download and install ---

download_and_install() {
  local archive="nostube-transcode-${TAG}-${PLATFORM}.tar.gz"
  local url="https://github.com/${REPO}/releases/download/${TAG}/${archive}"
  local tmpdir

  tmpdir=$(mktemp -d)
  trap 'rm -rf "${tmpdir:-}"' EXIT

  info "Downloading ${archive}..."
  if ! curl -sSfL -o "${tmpdir}/${archive}" "$url"; then
    error "Download failed. Check that version ${TAG} exists and has a ${PLATFORM} binary."
    error "URL: ${url}"
    exit 1
  fi

  info "Extracting..."
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  mkdir -p "${INSTALL_DIR}"
  info "Installing to ${INSTALL_DIR}/${BINARY_NAME}..."
  install -m 755 "${tmpdir}/nostube-transcode" "${INSTALL_DIR}/${BINARY_NAME}"

  info "Installed ${BINARY_NAME} ${TAG}"
}

# --- Check/install FFmpeg ---

check_ffmpeg() {
  local missing=()
  command -v ffmpeg  &>/dev/null || missing+=(ffmpeg)
  command -v ffprobe &>/dev/null || missing+=(ffprobe)

  if [ ${#missing[@]} -eq 0 ]; then
    info "FFmpeg is available."
    return
  fi

  warn "Missing: ${missing[*]}"

  local os
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"

  case "${os}" in
    linux)
      info "Installing FFmpeg via apt..."
      sudo apt-get update -qq && sudo apt-get install -y -qq ffmpeg
      ;;
    darwin)
      if command -v brew &>/dev/null; then
        info "Installing FFmpeg via Homebrew..."
        brew install ffmpeg
      else
        error "FFmpeg is required but Homebrew is not installed."
        echo "  Install Homebrew: https://brew.sh"
        echo "  Then run: brew install ffmpeg"
        exit 1
      fi
      ;;
  esac

  info "FFmpeg installed."
}

# --- OPERATOR_NPUB setup ---

setup_operator_npub() {
  mkdir -p "$DATA_DIR"

  # Check existing env file
  if [ -f "$ENV_FILE" ] && grep -q '^OPERATOR_NPUB=' "$ENV_FILE" 2>/dev/null; then
    local existing
    existing=$(grep '^OPERATOR_NPUB=' "$ENV_FILE" | cut -d= -f2-)
    if [ -n "$existing" ] && [ "$existing" != "npub1..." ]; then
      info "Found existing OPERATOR_NPUB in ${ENV_FILE}"
      return
    fi
  fi

  # Non-interactive (piped) mode — just warn
  if [ ! -t 0 ]; then
    warn "OPERATOR_NPUB not configured."
    echo "  Edit ${ENV_FILE} and add your npub:"
    echo "  echo 'OPERATOR_NPUB=npub1yourkey...' > ${ENV_FILE}"
    return
  fi

  echo ""
  bold "Your Nostr pubkey (npub) is required."
  echo "The DVM will only accept admin commands from this key."
  echo ""

  while true; do
    read -rp "Enter your OPERATOR_NPUB (npub1... or hex): " npub
    if [ -z "$npub" ]; then
      error "OPERATOR_NPUB is required. The DVM cannot start without it."
      continue
    fi
    if [[ "$npub" == npub1* ]] || [[ "$npub" =~ ^[0-9a-fA-F]{64}$ ]]; then
      break
    fi
    error "Invalid format. Must be an npub (npub1...) or 64-char hex pubkey."
  done

  echo "OPERATOR_NPUB=${npub}" > "$ENV_FILE"
  info "Wrote OPERATOR_NPUB to ${ENV_FILE}"
}

# --- Systemd service (Linux only) ---

setup_systemd() {
  local os
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  [ "$os" = "linux" ] || return 0

  local service_dir="${HOME}/.config/systemd/user"
  local service_file="${service_dir}/nostube-transcode.service"

  mkdir -p "$service_dir"

  cat > "$service_file" << EOF
[Unit]
Description=nostube-transcode DVM
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=${ENV_FILE}
ExecStart=${INSTALL_DIR}/${BINARY_NAME}
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
EOF

  info "Wrote systemd user service to ${service_file}"
}

# --- Summary ---

print_summary() {
  local os
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"

  echo ""
  bold "${BINARY_NAME} ${TAG} installed!"
  echo ""
  echo "  Binary:  ${INSTALL_DIR}/${BINARY_NAME}"
  echo "  Config:  ${ENV_FILE}"
  echo ""
  echo "  To start:   ${BINARY_NAME}"
  echo "  To upgrade:  re-run this install script"
  echo ""

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    warn "${INSTALL_DIR} is not in your PATH."
    echo "  Add it to your shell profile:"
    echo "    echo 'export PATH=\"\${HOME}/.local/bin:\${PATH}\"' >> ~/.bashrc"
    echo ""
  fi

  if [ "$os" = "linux" ]; then
    echo "  Systemd:  systemctl --user enable --now nostube-transcode"
    echo ""
  fi
}

# --- Main ---

main() {
  echo ""
  bold "nostube-transcode installer"
  echo ""

  detect_platform
  determine_version
  check_existing
  download_and_install
  check_ffmpeg
  setup_operator_npub
  setup_systemd
  print_summary
}

main "$@"
