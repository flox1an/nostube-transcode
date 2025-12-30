import type { Event, Filter } from "nostr-tools";
import { getPool } from "./client";
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
  // Check if this is a video transform DVM
  const dTag = event.tags.find((t) => t[0] === "d");
  if (!dTag || dTag[1] !== DVM_SERVICE_ID) {
    return null;
  }

  // Check if it supports our request kind
  const kTag = event.tags.find((t) => t[0] === "k");
  if (!kTag || kTag[1] !== KIND_DVM_REQUEST.toString()) {
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

  return {
    pubkey: event.pubkey,
    name: nameTag?.[1] || "Unknown DVM",
    about: aboutTag?.[1] || "",
    relays: relaysTag?.slice(1) || RELAYS,
    supportedModes: modeTag?.slice(2) || ["mp4", "hls"],
    supportedResolutions: resolutionTag?.slice(2) || ["360p", "720p", "1080p"],
    lastSeen: event.created_at,
  };
}

/**
 * Discover available video transform DVMs
 * Returns a list of DVM services, newest first
 */
export async function discoverDvms(
  timeoutMs: number = 5000
): Promise<DvmService[]> {
  const pool = getPool();

  const filter: Filter = {
    kinds: [KIND_DVM_ANNOUNCEMENT],
    "#d": [DVM_SERVICE_ID],
    "#k": [KIND_DVM_REQUEST.toString()],
  };

  return new Promise((resolve) => {
    const dvms = new Map<string, DvmService>();
    let resolved = false;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;

    const sub = pool.subscribeMany(RELAYS, filter, {
      onevent(event: Event) {
        // Ignore announcements older than 1 hour
        if (event.created_at < oneHourAgo) {
          return;
        }
        const dvm = parseAnnouncementEvent(event);
        if (dvm) {
          // Keep only the newest announcement per pubkey
          const existing = dvms.get(dvm.pubkey);
          if (!existing || dvm.lastSeen > existing.lastSeen) {
            dvms.set(dvm.pubkey, dvm);
          }
        }
      },
      oneose() {
        if (!resolved) {
          resolved = true;
          sub.close();
          // Sort by lastSeen descending (newest first)
          const result = Array.from(dvms.values()).sort(
            (a, b) => b.lastSeen - a.lastSeen
          );
          resolve(result);
        }
      },
    });

    // Timeout fallback
    setTimeout(() => {
      if (!resolved) {
        resolved = true;
        sub.close();
        const result = Array.from(dvms.values()).sort(
          (a, b) => b.lastSeen - a.lastSeen
        );
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
