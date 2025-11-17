A NOSTR DVM implmenting the NIP-90 DVM spec.

# DVM spec
https://github.com/nostr-protocol/nips/blob/master/90.md

# DVM example implementations
https://github.com/believethehype/nostrdvm
https://github.com/hzrd149/simple-count-dvm

There is no rust DVM library, i.e. we need to build upon a nostr (rust-nostr) library.

The nostube-transcode DVM should do video transcoding accorging to the following spec:



# Video Processing DVM - Technical Specification

## Overview
A Nostr-based Decentralized Virtual Machine (DVM) that accepts video URLs, processes them into HLS (HTTP Live Streaming) format with multiple resolutions, and uploads the results to Blossom servers.

## Architecture

### Core Components

1. **Nostr Event Listener**
   - Subscribes to multiple Nostr relays
   - Filters for kind `5207` events (DVM_VIDEO_TRANSFORM_REQUEST_KIND)
   - Maintains WebSocket connections with automatic reconnection
   - Deduplicates events using a seen set

2. **Video Processing Engine**
   - Downloads videos from URLs
   - Transcodes to HLS format with multiple resolutions
   - Generates content-addressable filenames (SHA-256 hashes)
   - Creates master and stream playlists

3. **Blossom Upload Client**
   - Uploads processed files to Blossom servers
   - Handles authentication via Nostr events (kind 24242)
   - Supports multiple upload servers
   - Manages blob lifecycle and cleanup

4. **Event Publisher**
   - Publishes status updates during processing
   - Returns results via kind `6207` events
   - Supports NIP-04 encryption for private jobs

## Event Protocol

### Request Event (Kind 5207)

```json
{
  "kind": 5207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["i", "<video_url>", "url"],
    ["relays", "wss://relay1.com", "wss://relay2.com"],
    ["output", "hls"],
    ["param", "<key>", "<value>"]
  ],
  "content": ""
}
```

**Required Tags:**
- `i` (input): `[tag_name, value, type, relay?, marker?]`
  - `value`: Video URL to process
  - `type`: Must be "url"

**Optional Tags:**
- `relays`: List of relays where results should be published
- `output`: Output format (currently "hls")
- `param`: Custom parameters for processing
- `encrypted`: Flag indicating NIP-04 encrypted content

### Result Event (Kind 6207)

```json
{
  "kind": 6207,
  "created_at": <unix_timestamp>,
  "tags": [
    ["request", "<stringified_request_event>"],
    ["e", "<request_event_id>"],
    ["p", "<requester_pubkey>"],
    ["i", "<original_input_url>", "url"],
    ["master", "<master_playlist_url>"],
    ["x", "<master_playlist_sha256>"],
    ["stream", "<stream_playlist_url>"],
    ["x", "<stream_playlist_sha256>"],
    ["segment", "<segment_url>"],
    ["x", "<segment_sha256>"],
    ...
  ],
  "content": ""
}
```

**Result Tags:**
- `request`: Stringified original request event
- `e`: Reference to request event ID
- `p`: Requester's public key
- `i`: Original input tag from request
- `master`: URL to master HLS playlist (master.m3u8)
- `stream`: URL to stream-specific playlist (one per resolution)
- `segment`: URL to video segment (multiple, one per segment)
- `x`: SHA-256 hash for each file (paired with master/stream/segment tags)

### Status Event (Kind 7000)

Published during processing to provide progress updates:

```json
{
  "kind": 7000,
  "created_at": <unix_timestamp>,
  "tags": [
    ["status", "<status_value>"],
    ["e", "<request_event_id>"],
    ["p", "<requester_pubkey>"],
    ["expiration", "<unix_timestamp>"]
  ],
  "content": "{\"msg\": \"Status message\"}"
}
```

**Status Values:**
- `payment-required`: Payment needed before processing
- `processing`: Job started
- `partial`: Intermediate update during processing
- `success`: Job completed successfully
- `error`: Job failed

## Video Processing Pipeline

### 1. Video Download
- Accept URL from request event's `i` tag
- Validate URL format
- Download video to temporary directory

### 2. FFmpeg Transcoding

**Default Configuration:**
```rust
struct TransformConfig {
    resolutions: HashMap<String, ResolutionConfig>,
    hls_time: u32,           // Default: 6 seconds
    hls_list_size: u32,      // Default: 0 (keep all)
    segment_type: String,    // Default: "fmp4"
}

struct ResolutionConfig {
    width: Option<u32>,
    height: Option<u32>,
    video_bitrate: Option<String>,
    audio_bitrate: Option<String>,
    video_codec: Option<String>,  // Default: "hevc_videotoolbox" or "libx265"
    audio_codec: Option<String>,  // Default: "copy"
    quality: Option<u32>,         // CRF value for quality
    is_original: bool,            // If true, copy original without transcoding
}
```

**Default Resolutions:**
- `360p`: 640x360, quality 50, audio 96k
- `720p`: 1280x720, quality 65
- `1080p`: Original stream (copy, no transcoding)

**FFmpeg Command Structure:**
```bash
ffmpeg -i <input_url> \
  # Complex filter for splitting and scaling
  -filter_complex "[0:v]split=2[360p][720p]; \
                   [360p]scale=640:360[360pout]; \
                   [720p]scale=1280:720[720pout]" \
  # Map video streams
  -map "[360pout]" -c:v:0 libx265 -q:v:0 50 \
  -map "[720pout]" -c:v:1 libx265 -q:v:1 65 \
  -map "0:v" -c:v:2 copy \
  # Map audio for each stream
  -map "0:a" -c:a:0 aac -b:a:0 96k \
  -map "0:a" -c:a:1 copy \
  -map "0:a" -c:a:2 copy \
  # HLS output options
  -f hls \
  -var_stream_map "v:0,a:0 v:1,a:1 v:2,a:2" \
  -hls_time 6 \
  -hls_list_size 0 \
  -hls_segment_type fmp4 \
  -master_pl_name master.m3u8 \
  stream_%v.m3u8
```

### 3. Content-Addressable Naming

After transcoding, rename all files based on their SHA-256 hash:

**For each stream playlist:**
1. Read playlist file (e.g., `stream_0.m3u8`)
2. For each segment reference in playlist:
   - Calculate SHA-256 hash of segment file
   - Rename segment: `<original>.m4s` → `<sha256>.m4s`
   - Update reference in playlist
3. Calculate SHA-256 of updated playlist content
4. Rename playlist: `stream_0.m3u8` → `<sha256>.m3u8`

**For master playlist:**
1. Update all stream references to use new SHA-256 names
2. Keep master playlist as `master.m3u8` (uploaded with this name)

**Playlist Update Logic:**
- Parse M3U8 files line by line
- Preserve HLS directives: `#EXTM3U`, `#EXT-X-VERSION`, `#EXT-X-TARGETDURATION`, etc.
- Update segment references: Both URI in `#EXT-X-MAP:URI="..."` and standalone filenames
- Handle both MPEG-TS (`.ts`) and fMP4 (`.m4s`) segments

### 4. Metadata Extraction

Extract and include in result tags:
```rust
struct VideoMetadata {
    duration: u32,        // Duration in seconds
    size: u64,            // File size in bytes
    width: u32,           // Video width
    height: u32,          // Video height
}
```

Use ffprobe:
```bash
ffprobe -v quiet -print_format json -show_format -show_streams "<url>"
```

Add to result tags:
- `["dim", "1920x1080"]`
- `["duration", "120"]`
- `["size", "50000000"]`

## Blossom Integration

### Authentication

Blossom uses Nostr event-based authentication (kind 24242):

**Upload Auth Event:**
```json
{
  "kind": 24242,
  "created_at": <unix_timestamp>,
  "content": "Upload",
  "tags": [
    ["t", "upload"],
    ["size", "<file_size_bytes>"],
    ["x", "<sha256_hash>"],
    ["name", "<filename>"],
    ["expiration", "<unix_timestamp>"]  // +10 minutes
  ]
}
```

**List Auth Event:**
```json
{
  "kind": 24242,
  "created_at": <unix_timestamp>,
  "content": "List Blobs",
  "tags": [
    ["t", "list"],
    ["expiration", "<unix_timestamp>"]
  ]
}
```

**Delete Auth Event:**
```json
{
  "kind": 24242,
  "created_at": <unix_timestamp>,
  "content": "Delete Blob",
  "tags": [
    ["t", "delete"],
    ["x", "<sha256_hash>"],
    ["expiration", "<unix_timestamp>"]
  ]
}
```

**Token Encoding:**
- Sign the auth event with DVM's private key
- Base64 encode: `btoa(JSON.stringify(signed_event))`
- Include in Authorization header: `Authorization: Nostr <base64_token>`

### Upload Protocol

**Endpoint:** `PUT {server}/upload`

**Headers:**
```
Content-Type: <mime_type>
Authorization: Nostr <base64_auth_token>
```

**Response:**
```json
{
  "url": "https://server.com/<sha256>.<ext>",
  "sha256": "<hash>",
  "size": <bytes>,
  "type": "<mime_type>",
  "created": <unix_timestamp>
}
```

**MIME Types:**
- `.m3u8`: `application/vnd.apple.mpegurl` (or `application/x-mpegURL`)
- `.m4s`: `video/iso.segment`
- `.ts`: `video/m2ts`
- `.mp4`: `video/mp4`

### List Blobs

**Endpoint:** `GET {server}/list/{pubkey}`

**Headers:**
```
Authorization: Nostr <base64_auth_token>
```

**Response:** Array of BlobDescriptor objects

### Delete Blob

**Endpoint:** `DELETE {server}/{sha256}`

**Headers:**
```
Authorization: Nostr <base64_auth_token>
```

### Cleanup Strategy

- Run cleanup every hour
- List all blobs owned by DVM's pubkey
- Delete blobs older than `BLOSSOM_BLOB_EXPIRATION_DAYS` (default: 30 days)
- Calculate cutoff: `now - (60 * 60 * 24 * EXPIRATION_DAYS)`
- For each blob where `blob.created < cutoff`: delete

## Encryption Support (NIP-04)

### Detecting Encrypted Requests

Request is encrypted if it has an `["encrypted"]` tag.

**Decryption:**
1. Check for `["encrypted"]` tag
2. Use NIP-04 to decrypt `content` field with requester's pubkey
3. Parse decrypted content as JSON array of tags
4. Merge decrypted tags with existing tags (excluding `e`, `p`, `encrypted`)
5. Process as normal request

**Encryption:**
If request was encrypted, encrypt the response:

1. Filter result tags (keep only `e` and `p` tags in plaintext)
2. Encrypt remaining tags as JSON array
3. Set encrypted array as `content`
4. Add `["encrypted"]` tag to result event

**Implementation:**
```rust
// NIP-04 uses AES-256-CBC with PKCS7 padding
// Shared secret from ECDH on secp256k1 curve
// IV is random 16 bytes, prepended to ciphertext
// Format: base64(iv + ciphertext) + "?iv=" + base64(iv)
```

## Configuration

### Environment Variables

```bash
# Required
NOSTR_PRIVATE_KEY=<hex_private_key>              # DVM's Nostr private key
NOSTR_RELAYS=wss://relay1.com,wss://relay2.com   # Comma-separated relay URLs
BLOSSOM_UPLOAD_SERVERS=https://server1.com,https://server2.com  # Upload targets

# Optional
BLOSSOM_BLOB_EXPIRATION_DAYS=30                  # Blob retention period (default: 30)
LNBITS_URL=<url>                                 # For payment integration (future)
LNBITS_ADMIN_KEY=<key>                          # For payment integration (future)
```

## Error Handling

### Job Rejection

Reject jobs in `shouldAcceptJob()` if:
- Missing `i` tag
- Input type is not "url"
- Invalid URL format
- Missing required parameters

Publish error status:
```rust
publish_status_event(
    context,
    "error",
    json!({"error": "Error message"}).to_string(),
    vec![],
    relays
);
```

### Processing Errors

If error occurs during processing:
1. Log error with context (request ID, error message)
2. Do NOT publish result event
3. Optionally publish error status event
4. Clean up temporary files
5. Continue listening for new requests

### Relay Connection Management

- Maintain persistent subscriptions to all configured relays
- Check subscription health every 30 seconds
- Reconnect closed/failed subscriptions automatically
- Log connection status changes
- Track subscriptions by relay URL

## State Management

### Deduplication
- Maintain in-memory `HashSet<String>` of seen event IDs
- Check before processing each request
- No persistence (resets on restart)
- Consider: Add time-based expiration for memory management

### Temporary Files
- Create unique temp directory per job: `temp{random_id}`
- Store all intermediate files in temp directory
- Delete entire temp directory after upload completes
- Location: Process working directory (configurable)

### Concurrent Jobs
- Current implementation: Sequential processing
- Consider: Job queue with configurable concurrency
- Consider: Rate limiting per requester pubkey

## Monitoring & Logging

### Debug Logging

Use namespace-based logging (like `debug` crate in Rust):
- `dvm:main` - Main event loop, subscriptions
- `dvm:ffmpeg` - Video processing operations
- `dvm:blossom` - Upload operations
- `dvm:nostr` - Event publishing

### Metrics to Track

- Jobs processed (total, success, error)
- Processing time per job
- Upload time per file
- Relay connection uptime
- Blob cleanup statistics

## Implementation Notes for Rust

### Recommended Crates

**Nostr:**
- `nostr-sdk` - Nostr protocol implementation
- `nostr` - Core types and utilities

**FFmpeg:**
- `ffmpeg-next` - Rust bindings for FFmpeg libraries
- Or: Use `std::process::Command` to spawn ffmpeg/ffprobe CLI

**HTTP Client:**
- `reqwest` - HTTP client for Blossom uploads
- Features: `stream`, `json`

**Crypto:**
- `sha2` - SHA-256 hashing
- `hex` - Hex encoding/decoding
- `base64` - Base64 encoding

**Async Runtime:**
- `tokio` - Async runtime with WebSocket support
- Features: `full`

**Utilities:**
- `serde` / `serde_json` - JSON serialization
- `dotenv` - Environment variable loading
- `log` / `env_logger` - Logging
- `chrono` - Timestamp handling

### Key Implementation Points

1. **Async Processing:**
   - Use async/await for network I/O (relays, uploads)
   - FFmpeg can be blocking - consider spawning tasks
   - Use channels for job queue if adding concurrency

2. **Error Propagation:**
   - Use `Result<T, E>` with custom error types
   - Implement `From` traits for error conversion
   - Use `?` operator for clean error propagation

3. **Resource Cleanup:**
   - Use RAII pattern with `Drop` trait
   - Consider `tempfile` crate for auto-cleanup
   - Ensure cleanup on panic using guards

4. **WebSocket Resilience:**
   - Handle connection drops gracefully
   - Implement exponential backoff for reconnection
   - Monitor subscription status

5. **SHA-256 Hashing:**
   - Stream large files rather than loading into memory
   - Use `Sha256::new()`, `update()`, `finalize()` pattern
   - Format as lowercase hex string

6. **M3U8 Parsing:**
   - Simple line-by-line processing (no complex parser needed)
   - Regex for URI extraction: `URI="([^"]+)"`
   - Preserve whitespace and comments

## Testing Strategy

### Unit Tests
- Blossom auth token generation
- M3U8 parsing and updating
- SHA-256 filename generation
- Event encryption/decryption
- DVM tag extraction (input, params, relays)

### Integration Tests
- Full video processing pipeline with sample video
- Blossom upload (mock server or test instance)
- Nostr event round-trip (mock relay)

### Manual Testing
- Process various video formats (mp4, webm, mkv)
- Test different resolutions and aspect ratios
- Verify HLS playback in video players
- Test encrypted requests
- Verify blob cleanup

## Performance Considerations

### Optimization Opportunities

1. **Parallel Uploads:**
   - Upload segments as they're created (don't wait for all)
   - Upload to multiple Blossom servers in parallel
   - Use concurrent streams with rate limiting

2. **FFmpeg Optimization:**
   - Use hardware acceleration where available
   - Tune codec settings for quality/speed tradeoff
   - Consider GPU encoding (NVENC, VideoToolbox)

3. **Memory Management:**
   - Stream file uploads rather than loading into memory
   - Limit concurrent jobs to prevent OOM
   - Monitor temp directory size

### Resource Limits

- Max video duration: Consider limiting (e.g., 1 hour)
- Max file size: Consider limiting (e.g., 5 GB)
- Concurrent jobs: Start with 1, increase based on resources
- Upload timeout: 10 minutes per file
- Processing timeout: Based on video duration

## Future Enhancements

From the TODO.md analysis:

1. **Dynamic Resolution Selection:**
   - Detect input video resolution
   - Only generate resolutions ≤ input resolution
   - Skip 1080p transcoding for 720p input

2. **Parameterized Configuration:**
   - Accept resolution preferences in request params
   - Configurable HLS segment length
   - Custom quality settings per request

3. **Payment Integration:**
   - Lightning payments via LNbits
   - Require payment before processing
   - Track payment status

4. **Progressive Upload:**
   - Start uploading segments as soon as ready
   - Publish partial results earlier
   - Improve perceived latency

5. **Resume Support:**
   - Handle large file uploads with resume capability
   - Avoid timeouts on slow connections

## Reference Implementation

This specification is based on the TypeScript implementation at:
- Repository: `dvm-video-processing`
- Main files:
  - `src/index.ts` - Main event loop and coordination (lines 1-448)
  - `src/helpers/ffmpeg.ts` - Video processing logic (lines 1-471)
  - `src/helpers/blossom.ts` - Blossom upload client (lines 1-148)
  - `src/helpers/dvm.ts` - DVM event parsing utilities (lines 1-32)
  - `src/const.ts` - Event kind constants (lines 1-7)
  - `src/env.ts` - Environment configuration (lines 1-34)
