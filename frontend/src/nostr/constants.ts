// Default relays for DVM discovery and requests
// Must match backend DEFAULT_BOOTSTRAP_RELAYS in src/bootstrap.rs
export const RELAYS = [
  "wss://relay.damus.io",
  "wss://nos.lol",
  "wss://relay.nostu.be",
];

// Index relays for NIP-65 relay list lookups
export const INDEX_RELAYS = ["wss://purplepag.es"];

// Always included in every DVM's relay list
export const DEFAULT_DVM_RELAY = "wss://relay.nostu.be";

// Nostr event kinds for DVM protocol
export const KIND_DVM_ANNOUNCEMENT = 31990;
export const KIND_DVM_REQUEST = 5207;
export const KIND_DVM_STATUS = 7000;
export const KIND_DVM_RESULT = 6207;

// DVM service identifier for video transformation
export const DVM_SERVICE_ID = "video-transform-hls";
