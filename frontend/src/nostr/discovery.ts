import type { Event, Filter } from "nostr-tools";
import { relayPool } from "./core";
import { RELAYS, KIND_DVM_ANNOUNCEMENT, KIND_DVM_REQUEST, DVM_SERVICE_ID } from "./constants";

/**
 * Represents a discovered DVM service
 */
export interface DvmService {
  pubkey: string;
  name: string;
  about: string;
  relays: string[];
  supportedModes: string[];
  supportedResolutions: string[];
  lastSeen: number;
}

/**
 * Parse a NIP-89 announcement event into a DvmService
 */
function parseAnnouncementEvent(event: Event): DvmService | null {
  console.log("[parseAnnouncementEvent] Parsing event:", event.id);
  
  // Check if this is a video transform DVM
  const dTag = event.tags.find((t) => t[0] === "d");
  console.log("[parseAnnouncementEvent] d tag:", dTag, "expected:", DVM_SERVICE_ID);
  if (!dTag || dTag[1] !== DVM_SERVICE_ID) {
    console.log("[parseAnnouncementEvent] Rejected: d tag mismatch");
    return null;
  }

  // Check if it supports our request kind
  const kTag = event.tags.find((t) => t[0] === "k");
  console.log("[parseAnnouncementEvent] k tag:", kTag, "expected:", KIND_DVM_REQUEST.toString());
  if (!kTag || kTag[1] !== KIND_DVM_REQUEST.toString()) {
    console.log("[parseAnnouncementEvent] Rejected: k tag mismatch");
    return null;
  }

  // Extract metadata
  const nameTag = event.tags.find((t) => t[0] === "name");
  const aboutTag = event.tags.find((t) => t[0] === "about");
  const relaysTag = event.tags.find((t) => t[0] === "relays");

  // Extract supported params
  const modeTag = event.tags.find(
    (t) => t[0] === "param" && t[1] === "mode"
  );
  const resolutionTag = event.tags.find(
    (t) => t[0] === "param" && t[1] === "resolution"
  );

  const result = {
    pubkey: event.pubkey,
    name: nameTag?.[1] || "Unknown DVM",
    about: aboutTag?.[1] || "",
    relays: relaysTag?.slice(1) || RELAYS,
    supportedModes: modeTag?.slice(2) || ["mp4", "hls"],
    supportedResolutions: resolutionTag?.slice(2) || ["360p", "720p", "1080p"],
    lastSeen: event.created_at,
  };
  
  console.log("[parseAnnouncementEvent] Parsed successfully:", result);
  return result;
}

/**
 * Discover available video transform DVMs
 * Returns a list of DVM services, newest first
 */
export async function discoverDvms(
  timeoutMs: number = 5000
): Promise<DvmService[]> {
  const filter: Filter = {
    kinds: [KIND_DVM_ANNOUNCEMENT],
    "#d": [DVM_SERVICE_ID],
    "#k": [KIND_DVM_REQUEST.toString()],
  };

  console.log("[DVM Discovery] Starting discovery with filter:", filter);
  console.log("[DVM Discovery] Searching relays:", RELAYS);
  console.log("[DVM Discovery] Current time:", Math.floor(Date.now() / 1000));
  console.log("[DVM Discovery] One hour ago:", Math.floor(Date.now() / 1000) - 3600);

  return new Promise((resolve) => {
    const dvms = new Map<string, DvmService>();
    let resolved = false;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    let eventCount = 0;
    let filteredOutCount = 0;
    let parsedCount = 0;

    const subscription = relayPool.subscription(RELAYS, filter).subscribe({
      next(response) {
        if (typeof response === 'string') {
          // EOSE signal
          console.log("[DVM Discovery] EOSE received from relay");
          if (!resolved) {
            resolved = true;
            subscription.unsubscribe();
            const result = Array.from(dvms.values()).sort(
              (a, b) => b.lastSeen - a.lastSeen
            );
            console.log(`[DVM Discovery] Complete: ${eventCount} events received, ${filteredOutCount} filtered out, ${parsedCount} parsed, ${result.length} DVMs found`);
            console.log("[DVM Discovery] Found DVMs:", result);
            resolve(result);
          }
          return;
        }

        // Event
        const event = response as Event;
        eventCount++;
        console.log(`[DVM Discovery] Event ${eventCount} received:`, {
          id: event.id,
          pubkey: event.pubkey,
          created_at: event.created_at,
          kind: event.kind,
          tags: event.tags,
        });

        // Ignore announcements older than 1 hour
        if (event.created_at < oneHourAgo) {
          filteredOutCount++;
          console.log(`[DVM Discovery] Event ${eventCount} filtered out (too old):`, event.created_at, "< ", oneHourAgo);
          return;
        }
        const dvm = parseAnnouncementEvent(event);
        console.log(`[DVM Discovery] Event ${eventCount} parsed result:`, dvm);
        if (dvm) {
          parsedCount++;
          // Keep only the newest announcement per pubkey
          const existing = dvms.get(dvm.pubkey);
          if (!existing || dvm.lastSeen > existing.lastSeen) {
            console.log(`[DVM Discovery] Adding/updating DVM:`, dvm.pubkey);
            dvms.set(dvm.pubkey, dvm);
          } else {
            console.log(`[DVM Discovery] Skipping older announcement for:`, dvm.pubkey);
          }
        }
      },
    });

    // Timeout fallback
    setTimeout(() => {
      if (!resolved) {
        resolved = true;
        subscription.unsubscribe();
        const result = Array.from(dvms.values()).sort(
          (a, b) => b.lastSeen - a.lastSeen
        );
        console.log(`[DVM Discovery] Timeout: ${eventCount} events received, ${filteredOutCount} filtered out, ${parsedCount} parsed, ${result.length} DVMs found`);
        console.log("[DVM Discovery] Found DVMs:", result);
        resolve(result);
      }
    }, timeoutMs);
  });
}

/**
 * Format pubkey for display (first 8 chars)
 */
export function formatPubkey(pubkey: string): string {
  return pubkey.slice(0, 8) + "...";
}
