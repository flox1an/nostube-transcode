import { useEffect, useRef, useState, useMemo } from "react";
import Hls from "hls.js";
import type { HlsConfig } from "hls.js";

interface VideoPlayerProps {
  src: string;
  /** Base64-encoded AES-128 encryption key from DVM result */
  encryptionKey?: string;
}

interface QualityLevel {
  height: number;
  index: number;
}

/** Decode base64 encryption key to Uint8Array */
function decodeEncryptionKey(base64Key: string): Uint8Array {
  const binaryString = atob(base64Key);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes;
}

/**
 * Creates a custom loader that intercepts key loading requests.
 * For our placeholder URI (urn:nostr:key), it provides the encryption key directly.
 * All other requests are delegated to the default loader.
 */
/* eslint-disable @typescript-eslint/no-explicit-any */
function createCustomLoader(encryptionKey: Uint8Array, DefaultLoader: any): any {
  return class CustomLoader {
    private loader: InstanceType<typeof DefaultLoader> | null = null;
    private _context: any = null;
    private _stats: any = null;
    private config: any;

    constructor(config: any) {
      this.config = config;
    }

    load(context: any, config: any, callbacks: any) {
      this._context = context;

      // Check if this is a key loading request with our placeholder URI
      const url = context.url || "";
      console.log("[HLS Loader] load() called - type:", context.type, "url:", JSON.stringify(url), "context keys:", Object.keys(context));

      // Check for our placeholder URI
      const isNostrKey = url === "urn:nostr:key" || url.startsWith("urn:nostr:");
      console.log("[HLS Loader] isNostrKey:", isNostrKey);

      if (isNostrKey) {
        console.log("[HLS Loader] Intercepting key request, providing key directly");
        // Provide the key directly without network request
        const now = performance.now();
        this._stats = {
          aborted: false,
          loaded: encryptionKey.length,
          total: encryptionKey.length,
          bwEstimate: 0,
          retry: 0,
          loading: { start: now, first: now, end: now },
          parsing: { start: now, end: now },
          buffering: { start: now, first: now, end: now },
        };

        // Simulate async response
        setTimeout(() => {
          callbacks.onSuccess(
            { data: encryptionKey.buffer },
            this._stats,
            context,
            null
          );
        }, 0);
        return;
      }

      // For all other requests, use the default loader
      this.loader = new DefaultLoader(this.config);
      this.loader.load(context, config, callbacks);
    }

    abort() {
      if (this.loader) {
        this.loader.abort();
      }
    }

    destroy() {
      if (this.loader) {
        this.loader.destroy();
      }
    }

    get stats() {
      if (this.loader?.stats) {
        return this.loader.stats;
      }
      if (this._stats) {
        return this._stats;
      }
      // Return default stats structure if nothing else available
      const now = performance.now();
      return {
        aborted: false,
        loaded: 0,
        total: 0,
        bwEstimate: 0,
        retry: 0,
        loading: { start: now, first: now, end: now },
        parsing: { start: now, end: now },
        buffering: { start: now, first: now, end: now },
      };
    }

    get context() {
      return this._context;
    }
  };
}
/* eslint-enable @typescript-eslint/no-explicit-any */

export function VideoPlayer({ src, encryptionKey }: VideoPlayerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [levels, setLevels] = useState<QualityLevel[]>([]);
  const [currentLevel, setCurrentLevel] = useState<number>(-1);
  const [selectedLevel, setSelectedLevel] = useState<number>(-1);

  // Decode encryption key once when it changes
  const decodedKey = useMemo(() => {
    if (!encryptionKey) return null;
    try {
      return decodeEncryptionKey(encryptionKey);
    } catch (e) {
      console.error("Failed to decode encryption key:", e);
      return null;
    }
  }, [encryptionKey]);

  const handleQualityChange = (level: number) => {
    setSelectedLevel(level);
    if (hlsRef.current) {
      hlsRef.current.currentLevel = level;
    }
  };

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    // Reset state on new src - this is intentional when source changes
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setError(null);
    setLevels([]);
    setCurrentLevel(-1);
    setSelectedLevel(-1);

    // Prefer hls.js when supported (gives us quality level control)
    // Only fall back to native HLS for browsers where hls.js doesn't work (e.g., iOS Safari)
    if (Hls.isSupported()) {
      const hlsConfig: Partial<HlsConfig> = {};

      // If we have an encryption key, use a custom loader to intercept key requests
      if (decodedKey) {
        console.log("[VideoPlayer] Encryption key detected, setting up custom loader. Key length:", decodedKey.length);
        const DefaultLoader = Hls.DefaultConfig.loader;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        hlsConfig.loader = createCustomLoader(decodedKey, DefaultLoader) as any;
      } else {
        console.log("[VideoPlayer] No encryption key, using default loader");
      }

      const hls = new Hls(hlsConfig);
      hlsRef.current = hls;

      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          setError("Failed to load video: " + data.details);
        }
      });

      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        const availableLevels = hls.levels
          .map((level, index) => ({
            height: level.height,
            index,
          }))
          .sort((a, b) => b.height - a.height);
        setLevels(availableLevels);
      });

      hls.on(Hls.Events.LEVEL_SWITCHED, (_event, data) => {
        setCurrentLevel(data.level);
      });

      hls.loadSource(src);
      hls.attachMedia(video);

      return () => {
        hlsRef.current = null;
        hls.destroy();
      };
    } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
      // Fallback to native HLS for iOS Safari
      // Note: Native HLS cannot inject keys, so encrypted content won't play
      if (encryptionKey) {
        setError("Encrypted HLS playback requires a compatible browser (Chrome, Firefox, Edge)");
        return;
      }
      video.src = src;
    } else {
      setError("HLS playback is not supported in this browser");
    }
  }, [src, decodedKey, encryptionKey]);

  if (error) {
    return (
      <div className="video-player video-error">
        <p>{error}</p>
        <p>
          <a href={src} target="_blank" rel="noopener noreferrer">
            Open video URL directly
          </a>
        </p>
      </div>
    );
  }

  const getLevelLabel = (level: QualityLevel) => `${level.height}p`;

  return (
    <div className="video-player">
      <video ref={videoRef} controls playsInline />
      {levels.length > 0 && (
        <div className="quality-selector">
          <select
            value={selectedLevel}
            onChange={(e) => handleQualityChange(Number(e.target.value))}
          >
            <option value={-1}>
              Auto{selectedLevel === -1 && currentLevel >= 0 ? ` (${levels.find(l => l.index === currentLevel)?.height}p)` : ""}
            </option>
            {levels.map((level) => (
              <option key={level.index} value={level.index}>
                {getLevelLabel(level)}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}
