# Multi-stage build for DVM Video Processing with Intel QSV support
# Optimized for UGREEN NAS (Intel N100, Pentium Gold 8505, i5-1235U)

# =============================================================================
# Stage 1: Build the frontend (React/Vite)
# =============================================================================
FROM node:20-slim AS frontend-builder

WORKDIR /app/frontend

# Copy frontend package files
COPY frontend/package*.json ./

# Install dependencies
RUN npm ci

# Copy frontend source
COPY frontend/ ./

# Build production bundle
RUN npm run build

# =============================================================================
# Stage 2: Build the Rust application
# =============================================================================
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir -p src frontend/dist && \
    echo "fn main() {}" > src/main.rs && \
    touch frontend/dist/.keep

# Build dependencies only
RUN cargo build --release && rm -rf src target/release/deps/dvm*

# Copy actual source code and frontend build
COPY src ./src
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist

# Build the application (embeds frontend via rust-embed)
RUN cargo build --release

# =============================================================================
# Stage 3: Runtime image with FFmpeg + Intel QSV (OneVPL for 12th gen+)
# =============================================================================
FROM debian:bookworm-slim

# Enable non-free-firmware for Intel media drivers
# Modify the DEB822 format sources file to include non-free components
RUN sed -i 's/^Components: main$/Components: main contrib non-free non-free-firmware/' /etc/apt/sources.list.d/debian.sources

# Install Intel Media drivers (VA-API), and FFmpeg with QSV support
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Intel GPU VA-API drivers (supports 5th gen through current)
    intel-media-va-driver-non-free \
    # VA-API libraries
    libva-drm2 \
    libva2 \
    vainfo \
    # FFmpeg with hardware acceleration support
    ffmpeg \
    # Runtime dependencies
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user with video/render group access for GPU
RUN groupadd -f video && \
    groupadd -f render && \
    useradd -m -u 1000 -G video,render dvm

WORKDIR /app

# Copy the built binary
COPY --from=builder /app/target/release/dvm-video-processing /usr/local/bin/dvm-video-processing

# Create temp directory with proper permissions
RUN mkdir -p /app/temp && chown -R dvm:dvm /app

# Switch to non-root user
USER dvm

# Environment defaults
ENV RUST_LOG=info
ENV TEMP_DIR=/app/temp
ENV HTTP_PORT=3000

EXPOSE 3000

# Healthcheck (simple TCP check since no /health endpoint)
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:3000/ || exit 1

CMD ["dvm-video-processing"]
