import type { EventTemplate } from "nostr-tools";
import { KIND_DVM_REQUEST, RELAYS } from "./constants";

export type OutputMode = "mp4" | "hls";
export type Resolution = "360p" | "480p" | "720p" | "1080p";

/** Stream playlist info for HLS output */
export interface StreamPlaylist {
  url: string;
  resolution: string;
  /** Total size of this stream (playlist + segments) in bytes */
  size_bytes: number;
}

/** DVM result for MP4 output */
export interface Mp4Result {
  type: "mp4";
  urls: string[];
  resolution: string;
  /** File size in bytes */
  size_bytes: number;
}

/** DVM result for HLS output */
export interface HlsResult {
  type: "hls";
  master_playlist: string;
  stream_playlists: StreamPlaylist[];
  /** Total size of all files in bytes */
  total_size_bytes: number;
}

/** Result from DVM job */
export type DvmResult = Mp4Result | HlsResult;

/**
 * Build a kind 5207 DVM video transform request event
 */
export function buildTransformRequest(
  videoUrl: string,
  dvmPubkey: string,
  dvmRelays: string[] = RELAYS,
  mode: OutputMode = "mp4",
  resolution: Resolution = "720p"
): EventTemplate {
  const tags: string[][] = [
    ["i", videoUrl, "url"],
    ["p", dvmPubkey],
    ["param", "mode", mode],
    ["relays", ...dvmRelays],
  ];

  // Only add resolution param for MP4 mode
  if (mode === "mp4") {
    tags.push(["param", "resolution", resolution]);
  }

  return {
    kind: KIND_DVM_REQUEST,
    content: "",
    created_at: Math.floor(Date.now() / 1000),
    tags,
  };
}

/**
 * Parse a status event (kind 7000) to extract status, message, and ETA
 */
export function parseStatusEvent(event: { tags: string[][] }): {
  status: string;
  message?: string;
  eta?: number;
} {
  let status = "unknown";
  let message: string | undefined;
  let eta: number | undefined;

  for (const tag of event.tags) {
    if (tag[0] === "status" && tag[1]) {
      status = tag[1];
    }
    if (tag[0] === "content" && tag[1]) {
      message = tag[1];
    }
    if (tag[0] === "eta" && tag[1]) {
      eta = parseInt(tag[1], 10);
    }
  }

  return { status, message, eta };
}

/**
 * Parse a result event (kind 6207) to extract the DVM result
 */
export function parseResultEvent(event: {
  content: string;
  tags: string[][];
}): DvmResult | null {
  // New format: result is JSON in content field
  if (event.content) {
    try {
      const result = JSON.parse(event.content) as DvmResult;
      if (result.type === "mp4" || result.type === "hls") {
        return result;
      }
    } catch {
      // Fall through to legacy format
    }
  }

  // Legacy format: URL in i tag
  for (const tag of event.tags) {
    if (tag[0] === "i" && tag[1] && tag[2] === "url") {
      // Return as HLS result for backwards compatibility
      return {
        type: "hls",
        master_playlist: tag[1],
        stream_playlists: [],
        total_size_bytes: 0,
      };
    }
  }
  return null;
}
