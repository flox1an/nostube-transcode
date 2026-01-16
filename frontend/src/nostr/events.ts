import type { EventTemplate } from "nostr-tools";
import type { ISigner } from "applesauce-signers";
import { KIND_DVM_REQUEST, RELAYS } from "./constants";

export type OutputMode = "mp4" | "hls";
export type Resolution = "360p" | "480p" | "720p" | "1080p";
export type Codec = "h264" | "h265";
export type HlsResolution = "240p" | "360p" | "480p" | "720p" | "1080p" | "original";

/** Check if content appears to be NIP-04 encrypted (base64?iv=base64 format) */
export function isNip04Encrypted(content: string): boolean {
  return content.includes("?iv=");
}

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
  /** Base64-encoded AES-128 encryption key (if encryption is enabled) */
  encryption_key?: string;
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
  resolution: Resolution = "720p",
  codec: Codec = "h264",
  hlsResolutions?: HlsResolution[],
  encryption?: boolean
): EventTemplate {
  const tags: string[][] = [
    ["i", videoUrl, "url"],
    ["p", dvmPubkey],
    ["param", "mode", mode],
    ["param", "codec", codec],
    ["relays", ...dvmRelays],
  ];

  // Only add resolution param for MP4 mode
  if (mode === "mp4") {
    tags.push(["param", "resolution", resolution]);
  }

  // Add HLS resolutions param for HLS mode
  if (mode === "hls" && hlsResolutions && hlsResolutions.length > 0) {
    tags.push(["param", "resolutions", hlsResolutions.join(",")]);
  }

  // Add encryption param for HLS mode (defaults to true if not specified)
  if (mode === "hls" && encryption !== undefined) {
    tags.push(["param", "encryption", encryption ? "true" : "false"]);
  }

  return {
    kind: KIND_DVM_REQUEST,
    content: "",
    created_at: Math.floor(Date.now() / 1000),
    tags,
  };
}

/**
 * Build an encrypted kind 5207 DVM video transform request event (NIP-04)
 * The input and params are encrypted in the content field
 */
export async function buildEncryptedTransformRequest(
  signer: ISigner,
  videoUrl: string,
  dvmPubkey: string,
  dvmRelays: string[] = RELAYS,
  mode: OutputMode = "mp4",
  resolution: Resolution = "720p",
  codec: Codec = "h264",
  hlsResolutions?: HlsResolution[],
  encryption?: boolean
): Promise<EventTemplate> {
  if (!signer.nip04) {
    throw new Error("Signer does not support NIP-04 encryption");
  }

  // Build the encrypted content JSON
  const encryptedContent: {
    i: string[];
    params: string[][];
  } = {
    i: [videoUrl, "url"],
    params: [
      ["param", "mode", mode],
      ["param", "codec", codec],
    ],
  };

  // Add resolution param for MP4 mode
  if (mode === "mp4") {
    encryptedContent.params.push(["param", "resolution", resolution]);
  }

  // Add HLS resolutions param for HLS mode
  if (mode === "hls" && hlsResolutions && hlsResolutions.length > 0) {
    encryptedContent.params.push(["param", "resolutions", hlsResolutions.join(",")]);
  }

  // Add encryption param for HLS mode
  if (mode === "hls" && encryption !== undefined) {
    encryptedContent.params.push(["param", "encryption", encryption ? "true" : "false"]);
  }

  // Encrypt the content with the DVM's public key
  const encryptedJson = await signer.nip04.encrypt(dvmPubkey, JSON.stringify(encryptedContent));

  // Build tags - only include public tags (p, relays, encrypted marker)
  const tags: string[][] = [
    ["p", dvmPubkey],
    ["relays", ...dvmRelays],
    ["encrypted"],
  ];

  return {
    kind: KIND_DVM_REQUEST,
    content: encryptedJson,
    created_at: Math.floor(Date.now() / 1000),
    tags,
  };
}

/**
 * Parse a status event (kind 7000) to extract status, message, and ETA
 * If the event is encrypted, pass signer and dvmPubkey to decrypt
 */
export async function parseStatusEvent(
  event: { content: string; tags: string[][] },
  signer?: ISigner,
  dvmPubkey?: string
): Promise<{
  status: string;
  message?: string;
  eta?: number;
}> {
  // Check if this is an encrypted status event
  const hasEncryptedTag = event.tags.some((t) => t[0] === "encrypted");

  if (hasEncryptedTag && event.content && signer?.nip04 && dvmPubkey) {
    try {
      const decrypted = await signer.nip04.decrypt(dvmPubkey, event.content);
      const parsed = JSON.parse(decrypted) as {
        status?: string;
        message?: string | null;
        eta?: number | null;
      };
      return {
        status: parsed.status || "unknown",
        message: parsed.message || undefined,
        eta: parsed.eta || undefined,
      };
    } catch (e) {
      console.error("Failed to decrypt status event:", e);
    }
  }

  // Unencrypted: parse from tags
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
 * If the event is encrypted, pass signer and dvmPubkey to decrypt
 */
export async function parseResultEvent(
  event: { content: string; tags: string[][] },
  signer?: ISigner,
  dvmPubkey?: string
): Promise<DvmResult | null> {
  let content = event.content;

  // Check if this is an encrypted result event
  const hasEncryptedTag = event.tags.some((t) => t[0] === "encrypted");

  if (hasEncryptedTag && content && signer?.nip04 && dvmPubkey) {
    try {
      content = await signer.nip04.decrypt(dvmPubkey, content);
    } catch (e) {
      console.error("Failed to decrypt result event:", e);
      return null;
    }
  }

  // Parse JSON content
  if (content) {
    try {
      const result = JSON.parse(content) as DvmResult;
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
