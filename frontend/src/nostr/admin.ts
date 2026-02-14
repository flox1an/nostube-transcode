import type { Event } from "nostr-tools";
import type { ISigner } from "applesauce-signers";
import { mapEventsToStore } from "applesauce-core";
import { filter } from "rxjs";
import { relayPool, eventStore } from "./core";

/** Admin RPC event kind (ephemeral) */
const ADMIN_RPC_KIND = 24207;

/** Generate a random hex ID for request correlation */
function randomId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

// Admin request (NIP-46 style wire format)
export interface AdminRequest {
  id: string;
  method: string;
  params: Record<string, unknown>;
}

// Admin response (NIP-46 style wire format)
export interface AdminResponseWire {
  id: string;
  result?: unknown;
  error?: string;
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
  paused: boolean;
  jobs_active: number;
  jobs_completed: number;
  jobs_failed: number;
  uptime_secs: number;
  hwaccel?: string;
  version: string;
}

export interface DvmDashboard {
  status: DvmStatus;
  config: DvmConfig;
  jobs: DvmJob[];
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
 * Returns the request ID for response correlation
 */
export async function sendAdminCommand(
  signer: ISigner,
  dvmPubkey: string,
  method: string,
  params: Record<string, unknown>,
  relays: string[]
): Promise<string> {
  if (!signer.nip44) {
    throw new Error("Signer does not support NIP-44 encryption");
  }

  const id = randomId();
  const adminPubkey = await signer.getPublicKey();
  const request: AdminRequest = { id, method, params };
  const content = JSON.stringify(request);
  const encrypted = await signer.nip44.encrypt(dvmPubkey, content);

  const now = Math.floor(Date.now() / 1000);

  const template: Event = {
    kind: ADMIN_RPC_KIND,
    pubkey: adminPubkey,
    created_at: now,
    tags: [
      ["p", dvmPubkey],
    ],
    content: encrypted,
    id: "",
    sig: "",
  };

  const signedEvent = await signer.signEvent(template);
  await relayPool.publish(relays, signedEvent);
  return id;
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
  onResponse: (response: AdminResponseWire) => void
): () => void {
  const filters = {
    kinds: [ADMIN_RPC_KIND],
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
        if (event.kind !== ADMIN_RPC_KIND) return;

        try {
          if (!signer.nip44) {
            console.error("Signer does not support NIP-44");
            return;
          }

          const decrypted = await signer.nip44.decrypt(
            dvmPubkey,
            event.content
          );
          const response = JSON.parse(decrypted) as AdminResponseWire;
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
