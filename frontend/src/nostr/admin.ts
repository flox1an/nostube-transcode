import type { Event } from "nostr-tools";
import type { ISigner } from "applesauce-signers";
import { mapEventsToStore } from "applesauce-core";
import { filter } from "rxjs";
import { relayPool, eventStore } from "./core";

/** Expiration time for admin commands (1 hour in seconds) */
const ADMIN_COMMAND_EXPIRATION_SECS = 3600;

// Admin command types
export type AdminCommand =
  | { cmd: "claim_admin"; secret: string }
  | { cmd: "get_config" }
  | { cmd: "set_relays"; relays: string[] }
  | { cmd: "set_blossom_servers"; servers: string[] }
  | { cmd: "set_blob_expiration"; days: number }
  | { cmd: "set_profile"; name: string; about: string }
  | { cmd: "pause" }
  | { cmd: "resume" }
  | { cmd: "status" }
  | { cmd: "job_history"; limit?: number }
  | { cmd: "self_test" }
  | { cmd: "system_info" };

// Admin response types
export interface AdminResponse {
  ok: boolean;
  error?: string;
  [key: string]: unknown;
}

export interface DvmConfig {
  relays: string[];
  blossom_servers: string[];
  blob_expiration_days: number;
  name: string;
  about: string;
  paused: boolean;
}

export interface DvmStatus {
  ok: boolean;
  paused: boolean;
  jobs_active: number;
  jobs_completed: number;
  jobs_failed: number;
  uptime_secs: number;
  hwaccel?: string;
  version: string;
}

export interface DvmJob {
  id: string;
  status: "completed" | "failed" | "processing";
  input_url: string;
  output_url?: string;
  started_at: number;
  completed_at?: number;
  duration_secs?: number;
}

export interface SelfTestResult {
  ok: boolean;
  success: boolean;
  video_duration_secs?: number;
  encode_time_secs?: number;
  speed_ratio?: number;
  speed_description?: string;
  hwaccel?: string;
  resolution?: string;
  output_size_bytes?: number;
  error?: string;
}

export interface HwEncoderInfo {
  name: string;
  selected: boolean;
  codecs: string[];
}

export interface GpuInfo {
  name: string;
  vendor: string;
  details?: string;
}

export interface DiskInfo {
  path: string;
  free_bytes: number;
  total_bytes: number;
  free_percent: number;
}

export interface FfmpegInfo {
  path: string;
  version?: string;
  ffprobe_path: string;
}

export interface SystemInfoResult {
  ok: boolean;
  platform: string;
  arch: string;
  hw_encoders: HwEncoderInfo[];
  gpu?: GpuInfo;
  disk: DiskInfo;
  ffmpeg: FfmpegInfo;
  temp_dir: string;
}

/**
 * Send an encrypted admin command to a DVM
 */
export async function sendAdminCommand(
  signer: ISigner,
  dvmPubkey: string,
  command: AdminCommand,
  relays: string[]
): Promise<void> {
  if (!signer.nip04) {
    throw new Error("Signer does not support NIP-04 encryption");
  }

  const adminPubkey = await signer.getPublicKey();
  const content = JSON.stringify(command);
  const encrypted = await signer.nip04.encrypt(dvmPubkey, content);

  const now = Math.floor(Date.now() / 1000);

  const template: Event = {
    kind: 4, // NIP-04 encrypted DM
    pubkey: adminPubkey,
    created_at: now,
    tags: [
      ["p", dvmPubkey],
      // NIP-40 expiration tag (24 hours)
      ["expiration", String(now + ADMIN_COMMAND_EXPIRATION_SECS)],
    ],
    content: encrypted,
    id: "",
    sig: "",
  };

  const signedEvent = await signer.signEvent(template);
  await relayPool.publish(relays, signedEvent);
}

/**
 * Subscribe to admin command responses from a DVM
 * Returns a function to unsubscribe
 * 
 * Uses EventStore for automatic deduplication across relays
 */
export function subscribeToAdminResponses(
  signer: ISigner,
  adminPubkey: string,
  dvmPubkey: string,
  relays: string[],
  onResponse: (response: AdminResponse) => void
): () => void {
  const filters = {
    kinds: [4], // Encrypted DMs
    authors: [dvmPubkey],
    "#p": [adminPubkey],
    since: Math.floor(Date.now() / 1000),
  };

  // Subscribe to relay pool and pipe through EventStore for deduplication
  const subscription = relayPool
    .subscription(relays, filters)
    .pipe(
      // Filter out EOSE signals
      filter((response): response is Event => 
        typeof response !== "string" && "kind" in response
      ),
      // Deduplicate events through EventStore
      mapEventsToStore(eventStore, true)
    )
    .subscribe({
      async next(event) {
        if (event.kind !== 4) return;

        try {
          if (!signer.nip04) {
            console.error("Signer does not support NIP-04");
            return;
          }

          const decrypted = await signer.nip04.decrypt(
            dvmPubkey,
            event.content
          );
          const response = JSON.parse(decrypted) as AdminResponse;
          onResponse(response);
        } catch (e) {
          console.error("Failed to decrypt admin response:", e);
        }
      },
    });

  return () => subscription.unsubscribe();
}

/**
 * Query for DVMs operated by a specific pubkey
 * Returns kind 31990 announcements with the operator tag
 * 
 * Uses EventStore for automatic deduplication across relays
 */
export function queryOperatorDvms(
  operatorPubkey: string,
  relays: string[],
  onDvm: (event: Event) => void
): () => void {
  const subscription = relayPool
    .subscription(relays, {
      kinds: [31990], // DVM announcements
      "#p": [operatorPubkey],
    })
    .pipe(
      // Filter out EOSE signals
      filter((response): response is Event => 
        typeof response !== "string" && "kind" in response
      ),
      // Deduplicate events through EventStore
      mapEventsToStore(eventStore, true),
      // Filter for operator tags
      filter((event) => {
        const pTags = event.tags.filter((t) => t[0] === "p");
        return pTags.some(
          (t) => t[1] === operatorPubkey && t[3] === "operator"
        );
      })
    )
    .subscribe({
      next(event) {
        onDvm(event as Event);
      },
    });

  return () => subscription.unsubscribe();
}

/**
 * Parse DVM announcement event to extract metadata
 */
export interface DvmMetadata {
  pubkey: string;
  name: string;
  about: string;
  supportedKinds: number[];
  operatorPubkey?: string;
  nip90Params?: unknown;
}

export function parseDvmAnnouncement(event: Event): DvmMetadata | null {
  if (event.kind !== 31990) return null;

  const nameTag = event.tags.find((t) => t[0] === "name");
  const aboutTag = event.tags.find((t) => t[0] === "about");
  const kTags = event.tags.filter((t) => t[0] === "k");
  const operatorTag = event.tags.find(
    (t) => t[0] === "p" && t[3] === "operator"
  );
  const paramsTag = event.tags.find((t) => t[0] === "nip90Params");

  return {
    pubkey: event.pubkey,
    name: nameTag?.[1] || "Unknown DVM",
    about: aboutTag?.[1] || "",
    supportedKinds: kTags.map((t) => parseInt(t[1], 10)),
    operatorPubkey: operatorTag?.[1],
    nip90Params: paramsTag?.[1] ? JSON.parse(paramsTag[1]) : undefined,
  };
}
