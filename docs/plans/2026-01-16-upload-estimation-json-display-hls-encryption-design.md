# Design: Upload Estimation, JSON Display, and HLS Encryption

**Date:** 2026-01-16

## Overview

Three features to improve the DVM:
1. Adaptive upload time estimation based on actual performance
2. Frontend raw JSON display for result inspection
3. AES-128 encryption for HLS output with key in playlist

---

## Feature 1: Adaptive Upload Time Estimation

### Problem
Current implementation uses hardcoded 5 MB/s upload speed estimate, which may be inaccurate depending on network conditions.

### Solution
Track actual upload speeds during the job and use a rolling average to dynamically update remaining time estimates.

### Data Structure

```rust
// In src/dvm/handler.rs

use std::collections::VecDeque;

struct UploadTracker {
    bytes_uploaded: u64,
    total_bytes: u64,
    start_time: Instant,
    recent_speeds: VecDeque<f64>,  // bytes per second, last 10 files
}

impl UploadTracker {
    const MAX_SAMPLES: usize = 10;
    const FALLBACK_SPEED: f64 = 5.0 * 1024.0 * 1024.0;  // 5 MB/s

    fn new(total_bytes: u64) -> Self;

    fn record_upload(&mut self, bytes: u64, duration_secs: f64) {
        self.bytes_uploaded += bytes;
        if duration_secs > 0.0 {
            let speed = bytes as f64 / duration_secs;
            self.recent_speeds.push_back(speed);
            if self.recent_speeds.len() > Self::MAX_SAMPLES {
                self.recent_speeds.pop_front();
            }
        }
    }

    fn average_speed(&self) -> f64 {
        if self.recent_speeds.is_empty() {
            Self::FALLBACK_SPEED
        } else {
            self.recent_speeds.iter().sum::<f64>() / self.recent_speeds.len() as f64
        }
    }

    fn estimated_remaining_secs(&self) -> u64 {
        let remaining_bytes = self.total_bytes.saturating_sub(self.bytes_uploaded);
        (remaining_bytes as f64 / self.average_speed()) as u64
    }

    fn current_speed_mbps(&self) -> f64 {
        self.average_speed() / (1024.0 * 1024.0)
    }
}
```

### Integration

1. Before upload loop, create `UploadTracker::new(total_bytes)`
2. After each file upload, measure duration and call `tracker.record_upload(file_size, duration)`
3. Progress updates query `tracker.estimated_remaining_secs()` for ETA
4. Initial estimate uses 5 MB/s fallback until first upload completes

### Files Changed
- `src/dvm/handler.rs` - Add `UploadTracker`, integrate into upload loops

---

## Feature 2: Frontend Raw JSON Display

### Problem
Users and developers cannot inspect the raw DVM result content for debugging or verification.

### Solution
Add a collapsible "Raw JSON" section below the existing structured result display.

### Component Changes

```tsx
// In frontend/src/App.tsx

const [showRawJson, setShowRawJson] = useState(false);

// After existing HLS/MP4 details:
{dvmResult && (
  <div className="raw-json-section">
    <button
      className="toggle-json-btn"
      onClick={() => setShowRawJson(!showRawJson)}
    >
      {showRawJson ? "Hide" : "Show"} Raw JSON
    </button>
    {showRawJson && (
      <pre className="json-display">
        {JSON.stringify(dvmResult, null, 2)}
      </pre>
    )}
  </div>
)}
```

### Styling

```css
/* In frontend/src/App.css */

.raw-json-section {
  margin-top: 1rem;
  padding-top: 1rem;
  border-top: 1px solid #333;
}

.toggle-json-btn {
  background: #2a2a2a;
  border: 1px solid #444;
  color: #888;
  padding: 0.5rem 1rem;
  font-family: monospace;
  cursor: pointer;
}

.toggle-json-btn:hover {
  background: #333;
  color: #aaa;
}

.json-display {
  background: #1a1a1a;
  padding: 1rem;
  border-radius: 4px;
  overflow-x: auto;
  max-height: 400px;
  overflow-y: auto;
  font-size: 0.85rem;
  color: #a0d0a0;
}
```

### Behavior
- Collapsed by default
- Shows parsed `dvmResult` object with 2-space indentation
- Scrollable both horizontally and vertically
- Max height of 400px

### Files Changed
- `frontend/src/App.tsx` - Add state and collapsible section
- `frontend/src/App.css` - Add styling

---

## Feature 3: HLS AES-128 Encryption

### Problem
HLS output is unencrypted, limiting content protection options.

### Solution
Generate a random AES-128 key, use FFmpeg's native HLS encryption, and embed the key as a data URI in stream playlists. Also include the key in the result JSON for client flexibility.

### Key Generation

```rust
// In src/video/transform.rs

use rand::RngCore;
use base64::{Engine, engine::general_purpose::STANDARD};

fn generate_aes_key() -> [u8; 16] {
    let mut key = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn key_to_base64(key: &[u8; 16]) -> String {
    STANDARD.encode(key)
}

fn key_to_data_uri(key: &[u8; 16]) -> String {
    format!("data:application/octet-stream;base64,{}", key_to_base64(key))
}
```

### FFmpeg Integration

FFmpeg requires a key info file with the format:
```
<key URI for playlist>
<path to key file>
```

```rust
// In src/video/ffmpeg.rs

// 1. Write key to temp file
let key_path = temp_dir.join("encryption.key");
std::fs::write(&key_path, &key)?;

// 2. Create key info file
let key_info_path = temp_dir.join("key_info.txt");
let key_uri = format!("data:application/octet-stream;base64,{}", base64_key);
let key_info_content = format!("{}\n{}", key_uri, key_path.display());
std::fs::write(&key_info_path, key_info_content)?;

// 3. Add to FFmpeg command
cmd.arg("-hls_key_info_file").arg(&key_info_path);
```

FFmpeg automatically adds to each stream playlist:
```
#EXT-X-KEY:METHOD=AES-128,URI="data:application/octet-stream;base64,<key>"
```

### Result JSON Changes

```rust
// In src/dvm/events.rs

#[derive(Serialize, Deserialize)]
pub struct HlsResult {
    pub master_playlist: String,
    pub stream_playlists: Vec<StreamPlaylist>,
    pub total_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_key: Option<String>,  // base64-encoded 16-byte key
}
```

### Frontend Type Update

```typescript
// In frontend/src/nostr/events.ts

interface HlsResult {
  type: "hls";
  master_playlist: string;
  stream_playlists: StreamPlaylist[];
  total_size_bytes: number;
  encryption_key?: string;  // base64-encoded key
}
```

### Player Compatibility
Standard HLS players (hls.js, Safari native, AVPlayer) automatically:
1. Parse `#EXT-X-KEY` tag from playlist
2. Decode data URI to get key bytes
3. Decrypt segments during playback

No frontend player changes required.

### Files Changed
- `src/video/transform.rs` - Key generation, pass to FFmpeg builder
- `src/video/ffmpeg.rs` - Write key files, add `-hls_key_info_file` arg
- `src/dvm/events.rs` - Add `encryption_key` field to `HlsResult`
- `src/dvm/handler.rs` - Pass key to result builder
- `frontend/src/nostr/events.ts` - Add `encryption_key` to type

---

## Error Handling

### Upload Tracker
- First upload may be unrepresentative; rolling average smooths this
- Failed uploads (before retry) are not recorded
- Falls back to 5 MB/s if no successful uploads yet

### AES Encryption
- Key file write failure fails the job (encryption is required)
- Temp files (key, key_info) cleaned up after upload
- MP4 output unchanged (no encryption)

### Frontend
- Missing `encryption_key` handled gracefully (older results)
- JSON display uses existing parsed state

---

## Future Considerations

The data URI approach for key storage allows future enhancement:
- Strip `#EXT-X-KEY` tags from playlists before upload
- Store key separately (in result JSON or key server)
- Build master playlist dynamically at runtime with injected key
- Enable per-user or time-limited key distribution
