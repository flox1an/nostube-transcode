import { SimplePool } from "nostr-tools";
import type { Event, Filter } from "nostr-tools";
import { ExtensionSigner } from "applesauce-signers";
import { KIND_DVM_STATUS, KIND_DVM_RESULT, RELAYS } from "./constants";
import { buildTransformRequest, type OutputMode, type Resolution, type Codec } from "./events";

// Singleton pool instance
let pool: SimplePool | null = null;

export function getPool(): SimplePool {
  if (!pool) {
    pool = new SimplePool();
  }
  return pool;
}

/**
 * Check if a NIP-07 extension is available
 */
export function hasExtension(): boolean {
  return typeof window !== "undefined" && "nostr" in window;
}

/**
 * Login with NIP-07 extension
 * Returns the user's public key hex
 */
export async function login(): Promise<string> {
  if (!hasExtension()) {
    throw new Error("No Nostr extension found. Install Alby or nos2x.");
  }

  const signer = new ExtensionSigner();
  const pubkey = await signer.getPublicKey();
  return pubkey;
}

export interface PublishResult {
  eventId: string;
  signedEvent: Event;
}

/**
 * Sign and publish a video transform request
 * Returns the event ID and signed event
 */
export async function publishTransformRequest(
  videoUrl: string,
  dvmPubkey: string,
  dvmRelays: string[] = RELAYS,
  mode: OutputMode = "mp4",
  resolution: Resolution = "720p",
  codec: Codec = "h264"
): Promise<PublishResult> {
  if (!hasExtension()) {
    throw new Error("No Nostr extension available");
  }

  const signer = new ExtensionSigner();
  const template = buildTransformRequest(videoUrl, dvmPubkey, dvmRelays, mode, resolution, codec);
  const signedEvent = await signer.signEvent(template);

  const p = getPool();
  // Publish to both the DVM's relays and default relays
  const allRelays = [...new Set([...dvmRelays, ...RELAYS])];
  const promises = p.publish(allRelays, signedEvent);

  // Wait for at least one relay to accept
  await Promise.any(promises);

  return {
    eventId: signedEvent.id,
    signedEvent,
  };
}

export interface DvmResponse {
  type: "status" | "result";
  event: Event;
}

/**
 * Subscribe to DVM responses for a specific request
 * Returns a function to unsubscribe
 */
export function subscribeToResponses(
  requestEventId: string,
  dvmPubkey: string,
  dvmRelays: string[] = RELAYS,
  onEvent: (response: DvmResponse) => void
): () => void {
  const p = getPool();

  const filter: Filter = {
    kinds: [KIND_DVM_STATUS, KIND_DVM_RESULT],
    "#e": [requestEventId],
    authors: [dvmPubkey],
  };

  // Subscribe to both DVM's relays and default relays
  const allRelays = [...new Set([...dvmRelays, ...RELAYS])];

  const sub = p.subscribeMany(allRelays, filter, {
    onevent(event: Event) {
      if (event.kind === KIND_DVM_STATUS) {
        onEvent({ type: "status", event });
      } else if (event.kind === KIND_DVM_RESULT) {
        onEvent({ type: "result", event });
      }
    },
  });

  return () => sub.close();
}
