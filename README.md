# nostube-transcode

A Nostr-based Decentralized Virtual Machine (DVM) for video transcoding. This DVM accepts video URLs via Nostr events, processes them into either HLS (HTTP Live Streaming) or MP4 format with configurable resolutions, and uploads the results to Blossom servers.

## Features

- **Nostr Integration**: Listens for DVM video transform requests (kind 5207) on multiple Nostr relays
- **Multiple Output Formats**:
  - **HLS**: Adaptive bitrate streaming with master playlist and segments
  - **MP4**: Single-file video output with fast start
- **Configurable Resolutions**: Support for 480p, 720p, and 1080p (multiple selections allowed)
- **H.265 Encoding**: Efficient video compression using libx265 codec
- **Content-Addressable Storage**: Uses SHA-256 hashes for file naming
- **Blossom Upload**: Automatically uploads processed files to Blossom servers
- **Status Updates**: Publishes processing status updates (kind 7000) during job execution
- **Automatic Cleanup**: Periodically removes old blobs from Blossom servers

## Requirements

- Rust 1.70 or higher
- FFmpeg (with libx265 support)
- FFprobe

### Installing FFmpeg

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install ffmpeg
```

**macOS:**
```bash
brew install ffmpeg
```

**Verify installation:**
```bash
ffmpeg -version
ffprobe -version
```

## Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/nostube-transcode.git
cd nostube-transcode
```

2. Copy the example environment file:
```bash
cp .env.example .env
```

3. Edit `.env` and configure your settings:
```bash
# Required settings
NOSTR_PRIVATE_KEY=your_private_key_in_hex
NOSTR_RELAYS=wss://relay.damus.io,wss://relay.nostr.band
BLOSSOM_UPLOAD_SERVERS=https://your-blossom-server.com

# Optional settings
BLOSSOM_BLOB_EXPIRATION_DAYS=30
```

4. Build the project:
```bash
cargo build --release
```

## Usage

Run the DVM:
```bash
cargo run --release
```

The DVM will:
1. Connect to configured Nostr relays
2. Subscribe to video transform requests (kind 5207)
3. Process incoming requests automatically
4. Upload results to Blossom servers
5. Publish results back to Nostr (kind 6207)

## Configuration

### Environment Variables

| Variable | Required | Description | Default |
|----------|----------|-------------|---------|
| `NOSTR_PRIVATE_KEY` | Yes | Your DVM's Nostr private key (hex format) | - |
| `NOSTR_RELAYS` | Yes | Comma-separated list of Nostr relay URLs | - |
| `BLOSSOM_UPLOAD_SERVERS` | Yes | Comma-separated list of Blossom server URLs | - |
| `BLOSSOM_BLOB_EXPIRATION_DAYS` | No | Days to keep blobs before cleanup | 30 |
| `LNBITS_URL` | No | LNbits instance URL for payments | - |
| `LNBITS_ADMIN_KEY` | No | LNbits admin key | - |

### Video Processing

The DVM supports two output formats:

#### HLS (HTTP Live Streaming)
Generates adaptive bitrate streaming with multiple resolutions:
- Segment duration: 6 seconds
- Segment type: fMP4
- Playlist size: unlimited (keeps all segments)
- Codec: H.265 (libx265), AAC audio at 128k

#### MP4 (Single File)
Generates standalone MP4 files for each requested resolution:
- Codec: H.265 (libx265) with CRF 28
- Audio: AAC at 128k
- Fast start enabled (moov atom at beginning)

#### Supported Resolutions
- **480p**: 854x480
- **720p**: 1280x720
- **1080p**: 1920x1080

#### Default Behavior
If no parameters are specified:
- Output format: HLS
- Resolutions: 480p, 720p, 1080p (all three)

## Event Protocol

### Request Event (Kind 5207)

**Basic HLS Request (default - all resolutions):**
```json
{
  "kind": 5207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["i", "<video_url>", "url"],
    ["relays", "wss://relay1.com", "wss://relay2.com"]
  ],
  "content": ""
}
```

**MP4 Request with specific resolutions:**
```json
{
  "kind": 5207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["i", "<video_url>", "url"],
    ["param", "output", "mp4"],
    ["param", "resolution", "720p"],
    ["param", "resolution", "1080p"]
  ],
  "content": ""
}
```

**HLS Request with single resolution:**
```json
{
  "kind": 5207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["i", "<video_url>", "url"],
    ["param", "output", "hls"],
    ["param", "resolution", "720p"]
  ],
  "content": ""
}
```

**Request Parameters:**
- `["param", "output", "hls|mp4"]` - Output format (default: hls)
- `["param", "resolution", "480p|720p|1080p"]` - Resolution (can specify multiple times)
- `["relays", "wss://relay1", ...]` - Relays for publishing results

### Result Event (Kind 6207)

**HLS Result:**
```json
{
  "kind": 6207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["e", "<request_event_id>"],
    ["p", "<requester_pubkey>"],
    ["i", "<original_input_url>", "url"],
    ["output", "hls"],
    ["master", "<master_playlist_url>"],
    ["x", "<master_playlist_sha256>"],
    ["stream", "<stream_playlist_url>"],
    ["x", "<stream_playlist_sha256>"],
    ["segment", "<segment_url>"],
    ["x", "<segment_sha256>"],
    ... (more segments)
    ["dim", "1920x1080"],
    ["duration", "120"],
    ["size", "50000000"]
  ],
  "content": ""
}
```

**MP4 Result:**
```json
{
  "kind": 6207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["e", "<request_event_id>"],
    ["p", "<requester_pubkey>"],
    ["i", "<original_input_url>", "url"],
    ["output", "mp4"],
    ["video", "<720p_video_url>", "720p"],
    ["x", "<720p_sha256>"],
    ["video", "<1080p_video_url>", "1080p"],
    ["x", "<1080p_sha256>"],
    ["dim", "1920x1080"],
    ["duration", "120"],
    ["size", "50000000"]
  ],
  "content": ""
}
```

**Result Tags:**
- `e`: Reference to request event ID
- `p`: Requester's public key
- `i`: Original input tag from request
- `output`: Output format used (hls or mp4)
- For HLS:
  - `master`: URL to master HLS playlist (master.m3u8)
  - `stream`: URL to stream-specific playlist (one per resolution)
  - `segment`: URL to video segment (multiple, one per segment)
- For MP4:
  - `video`: URL to MP4 file, with resolution as second value (e.g., "720p")
- `x`: SHA-256 hash for each file (paired with master/stream/segment/video tags)
- `dim`: Original video dimensions
- `duration`: Video duration in seconds
- `size`: Original file size in bytes

### Status Event (Kind 7000)

Published during processing:

```json
{
  "kind": 7000,
  "created_at": <unix_timestamp>,
  "tags": [
    ["status", "processing"],
    ["e", "<request_event_id>"],
    ["p", "<requester_pubkey>"]
  ],
  "content": "{\"msg\": \"Starting video processing\"}"
}
```

Status values:
- `processing`: Job started
- `success`: Job completed successfully
- `error`: Job failed

## Architecture

```
┌─────────────────┐
│  Nostr Relays   │
│   (Kind 5207)   │
└────────┬────────┘
         │
         ▼
┌─────────────────────┐
│   Event Listener    │
│  (Deduplication)    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Video Processor     │
│  - Download         │
│  - FFmpeg transcode │
│  - HLS generation   │
│  - SHA-256 naming   │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│  Blossom Client     │
│  - Auth (kind 24242)│
│  - Upload files     │
│  - Cleanup old blobs│
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│  Event Publisher    │
│   (Kind 6207)       │
└─────────────────────┘
```

## Development

### Project Structure

```
nostube-transcode/
├── src/
│   ├── main.rs              # Main event loop and coordination
│   ├── const.rs             # Event kind constants
│   ├── env.rs               # Configuration management
│   └── helpers/
│       ├── mod.rs           # Module exports
│       ├── dvm.rs           # DVM utility functions
│       ├── blossom.rs       # Blossom upload client
│       └── ffmpeg.rs        # Video processing engine
├── Cargo.toml               # Rust dependencies
├── .env.example             # Example configuration
├── spec.md                  # Technical specification
└── README.md                # This file
```

### Building from Source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests (when implemented)
cargo test

# Check code without building
cargo check
```

### Logging

The DVM uses `env_logger` for logging. Set the `RUST_LOG` environment variable to control log levels:

```bash
# Info level (default)
RUST_LOG=info cargo run

# Debug level (verbose)
RUST_LOG=debug cargo run

# Trace level (very verbose)
RUST_LOG=trace cargo run
```

## NIP-90 Compliance

This DVM implements [NIP-90: Data Vending Machines](https://github.com/nostr-protocol/nips/blob/master/90.md).

- Subscribes to kind 5207 (DVM video transform request)
- Publishes kind 6207 (DVM video transform result)
- Publishes kind 7000 (DVM status update)
- Supports relay hints via `relays` tag
- Handles input via `i` tag with type "url"

## Blossom Integration

The DVM uses [Blossom](https://github.com/hzrd149/blossom) for decentralized file storage:

- Authentication via Nostr events (kind 24242)
- Content-addressable storage using SHA-256
- Automatic cleanup of old files
- Multiple server support

## Security Considerations

- Keep your `NOSTR_PRIVATE_KEY` secure and never commit it to version control
- The `.env` file is in `.gitignore` to prevent accidental commits
- Use HTTPS for Blossom servers to ensure encrypted uploads
- Regularly update dependencies: `cargo update`

## Performance

- Processing is done sequentially (one job at a time)
- Temporary files are automatically cleaned up after upload
- Blob cleanup runs hourly to free up storage space
- FFmpeg uses H.265 for better compression (slower encoding)

## Troubleshooting

### FFmpeg not found
```
Error: FFmpeg command not found
```
Install FFmpeg and ensure it's in your PATH.

### Blossom upload fails
```
Error: Upload failed with status 401
```
Check that your `NOSTR_PRIVATE_KEY` is valid and the Blossom server accepts your uploads.

### Relay connection issues
```
Error: Failed to connect to relay
```
Verify relay URLs are correct and accessible. Try different relays if needed.

## Future Enhancements

- Dynamic resolution selection based on input video
- Configurable processing parameters via request tags
- Lightning payment integration
- Concurrent job processing
- Resume support for large uploads
- NIP-04 encryption support for private jobs

## License

MIT License - see LICENSE file for details

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## References

- [NIP-90: Data Vending Machines](https://github.com/nostr-protocol/nips/blob/master/90.md)
- [Blossom Specification](https://github.com/hzrd149/blossom)
- [rust-nostr](https://github.com/rust-nostr/nostr)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
