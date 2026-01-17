import type { Event, Filter } from "nostr-tools";
import type { ISigner } from "applesauce-signers";
import { KIND_DVM_STATUS, KIND_DVM_RESULT, RELAYS } from "./constants";
import { buildTransformRequest, buildEncryptedTransformRequest, type OutputMode, type Resolution, type Codec, type HlsResolution } from "./events";
import { relayPool } from "./core";
import { accountManager } from "../providers/AppProviders";

export interface PublishResult {
  eventId: string;
  signedEvent: Event;
}

/**
 * Get the current signer from AccountManager
 */
export function getCurrentSigner(): ISigner | undefined {
  return accountManager.signer;
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

  // Publish to both the DVM's relays and default relays
  const allRelays = [...new Set([...dvmRelays, ...RELAYS])];
  await relayPool.publish(allRelays, signedEvent);

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
  const filter: Filter = {
    kinds: [KIND_DVM_STATUS, KIND_DVM_RESULT],
    "#e": [requestEventId],
    authors: [dvmPubkey],
  };

  // Subscribe to both DVM's relays and default relays
  const allRelays = [...new Set([...dvmRelays, ...RELAYS])];

  const subscription = relayPool.subscription(allRelays, filter).subscribe({
    next(event) {
      if (typeof event === 'string') return;
      if (!('kind' in event)) return;

      if (event.kind === KIND_DVM_STATUS) {
        onEvent({ type: "status", event: event as Event });
      } else if (event.kind === KIND_DVM_RESULT) {
        onEvent({ type: "result", event: event as Event });
      }
    },
  });

  return () => subscription.unsubscribe();
}
