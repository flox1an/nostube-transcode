import { SimplePool, nip19 } from "nostr-tools";
import type { Event, Filter } from "nostr-tools";
import { ExtensionSigner, SimpleSigner, NostrConnectSigner } from "applesauce-signers";
import type { ISigner } from "applesauce-signers";
import { KIND_DVM_STATUS, KIND_DVM_RESULT, RELAYS } from "./constants";
import { buildTransformRequest, buildEncryptedTransformRequest, type OutputMode, type Resolution, type Codec, type HlsResolution } from "./events";

// Singleton pool instance
let pool: SimplePool | null = null;

// Current signer instance (used for signing events)
let currentSigner: ISigner | null = null;

export type LoginMethod = "extension" | "nsec" | "bunker";

export interface LoginResult {
  pubkey: string;
  method: LoginMethod;
}

export function getPool(): SimplePool {
  if (!pool) {
    pool = new SimplePool();
  }
  return pool;
}

/**
 * Get the current signer (for signing events)
 */
export function getCurrentSigner(): ISigner | null {
  return currentSigner;
}

/**
 * Check if a NIP-07 extension is available
 */
export function hasExtension(): boolean {
  return typeof window !== "undefined" && "nostr" in window;
}

/**
 * Login with NIP-07 extension
 */
export async function loginWithExtension(): Promise<LoginResult> {
  if (!hasExtension()) {
    throw new Error("No Nostr extension found. Install Alby or nos2x.");
  }

  const signer = new ExtensionSigner();
  const pubkey = await signer.getPublicKey();
  currentSigner = signer;
  return { pubkey, method: "extension" };
}

/**
 * Login with nsec (secret key)
 */
export async function loginWithNsec(nsec: string): Promise<LoginResult> {
  const decoded = nip19.decode(nsec);
  if (decoded.type !== "nsec") {
    throw new Error("Invalid nsec format");
  }

  const signer = new SimpleSigner(decoded.data);
  const pubkey = await signer.getPublicKey();
  currentSigner = signer;
  return { pubkey, method: "nsec" };
}

/**
 * Login with bunker URI (NIP-46)
 */
export async function loginWithBunker(bunkerUri: string): Promise<LoginResult> {
  if (!bunkerUri.startsWith("bunker://")) {
    throw new Error("Invalid bunker URI");
  }

  const signer = await NostrConnectSigner.fromBunkerURI(bunkerUri);
  const pubkey = await signer.getPublicKey();
  currentSigner = signer;
  return { pubkey, method: "bunker" };
}

/**
 * Logout - clear the current signer
 */
export function logout(): void {
  currentSigner = null;
}

/**
 * Legacy login function for backward compatibility
 * @deprecated Use loginWithExtension instead
 */
export async function login(): Promise<string> {
  const result = await loginWithExtension();
  return result.pubkey;
}

export interface PublishResult {
  eventId: string;
  signedEvent: Event;
}

/**
 * Sign and publish a video transform request
 * Uses NIP-04 encryption if the signer supports it
 * Returns the event ID and signed event
 */
export async function publishTransformRequest(
  videoUrl: string,
  dvmPubkey: string,
  dvmRelays: string[] = RELAYS,
  mode: OutputMode = "mp4",
  resolution: Resolution = "720p",
  codec: Codec = "h264",
  hlsResolutions?: HlsResolution[],
  encryption?: boolean
): Promise<PublishResult> {
  const signer = getCurrentSigner();
  if (!signer) {
    throw new Error("Not logged in. Please login first.");
  }

  // Use encrypted request if signer supports NIP-04
  let template;
  if (signer.nip04) {
    template = await buildEncryptedTransformRequest(
      signer,
      videoUrl,
      dvmPubkey,
      dvmRelays,
      mode,
      resolution,
      codec,
      hlsResolutions,
      encryption
    );
  } else {
    template = buildTransformRequest(videoUrl, dvmPubkey, dvmRelays, mode, resolution, codec, hlsResolutions, encryption);
  }

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
