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

  # Check if we can prompt the user (works even when piped via curl | bash)
  if [ ! -t 0 ] && [ ! -e /dev/tty ]; then
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
    read -rp "Enter your OPERATOR_NPUB (npub1... or hex): " npub < /dev/tty
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

# --- Daemon setup (systemd on Linux, launchd on macOS) ---

setup_daemon() {
  local os
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"

  case "${os}" in
    linux)
      if pidof systemd &>/dev/null || [ -d /run/systemd/system ]; then
        setup_systemd
      else
        setup_sysvinit
      fi
      ;;
    darwin) setup_launchd ;;
  esac
}

setup_systemd() {
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

setup_sysvinit() {
  local init_script="${DATA_DIR}/nostube-transcode.initd"
  local run_user
  run_user="$(whoami)"

  cat > "$init_script" << INITEOF
#!/bin/sh
### BEGIN INIT INFO
# Provides:          nostube-transcode
# Required-Start:    \$network \$remote_fs
# Required-Stop:     \$network \$remote_fs
# Default-Start:     2 3 4 5
# Default-Stop:      0 1 6
# Short-Description: nostube-transcode DVM
# Description:       Nostr Data Vending Machine for video transcoding
### END INIT INFO

DAEMON="${INSTALL_DIR}/${BINARY_NAME}"
DAEMON_USER="${run_user}"
PIDFILE="/var/run/nostube-transcode.pid"
ENV_FILE="${ENV_FILE}"

# Source environment
if [ -f "\$ENV_FILE" ]; then
  set -a
  . "\$ENV_FILE"
  set +a
fi

case "\$1" in
  start)
    echo "Starting nostube-transcode..."
    if [ -f "\$PIDFILE" ] && kill -0 \$(cat "\$PIDFILE") 2>/dev/null; then
      echo "Already running (pid \$(cat "\$PIDFILE"))"
      exit 0
    fi
    start-stop-daemon --start --background --make-pidfile --pidfile "\$PIDFILE" \\
      --chuid "\$DAEMON_USER" --exec "\$DAEMON"
    echo "Started."
    ;;
  stop)
    echo "Stopping nostube-transcode..."
    if [ -f "\$PIDFILE" ]; then
      start-stop-daemon --stop --pidfile "\$PIDFILE" --retry 10
      rm -f "\$PIDFILE"
      echo "Stopped."
    else
      echo "Not running."
    fi
    ;;
  restart)
    \$0 stop
    sleep 1
    \$0 start
    ;;
  status)
    if [ -f "\$PIDFILE" ] && kill -0 \$(cat "\$PIDFILE") 2>/dev/null; then
      echo "nostube-transcode is running (pid \$(cat "\$PIDFILE"))"
    else
      echo "nostube-transcode is not running"
      exit 1
    fi
    ;;
  *)
    echo "Usage: \$0 {start|stop|restart|status}"
    exit 1
    ;;
esac
INITEOF

  chmod 755 "$init_script"
  info "Generated SysV init script at ${init_script}"
}

setup_launchd() {
  local plist_dir="${HOME}/Library/LaunchAgents"
  local plist_file="${plist_dir}/com.nostube.transcode.plist"

  mkdir -p "$plist_dir"

  cat > "$plist_file" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.nostube.transcode</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/${BINARY_NAME}</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>${INSTALL_DIR}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>${HOME}/.local/share/nostube-transcode/stdout.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/.local/share/nostube-transcode/stderr.log</string>
</dict>
</plist>
EOF

  info "Wrote launchd plist to ${plist_file}"
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
    if pidof systemd &>/dev/null || [ -d /run/systemd/system ]; then
      echo "  Run as daemon:"
      echo "    systemctl --user enable --now nostube-transcode"
      echo ""
    else
      echo "  Install as system service:"
      echo "    sudo cp ${DATA_DIR}/nostube-transcode.initd /etc/init.d/nostube-transcode"
      echo "    sudo update-rc.d nostube-transcode defaults"
      echo "    sudo service nostube-transcode start"
      echo ""
    fi
  elif [ "$os" = "darwin" ]; then
    echo "  Run as daemon:"
    echo "    launchctl load ~/Library/LaunchAgents/com.nostube.transcode.plist"
    echo ""
    echo "  Stop daemon:"
    echo "    launchctl unload ~/Library/LaunchAgents/com.nostube.transcode.plist"
    echo ""
    echo "  Logs:"
    echo "    tail -f ~/.local/share/nostube-transcode/stderr.log"
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
  setup_daemon
  print_summary
}

main "$@"
