# ContextVM Research

> Research conducted 2026-03-08. Source: GitHub repos, SDK source code, CEP specifications.

## What is ContextVM?

ContextVM (CVM) is a protocol that uses **Nostr as a transport layer for MCP** (Model Context Protocol). Instead of HTTP/SSE/stdio, MCP JSON-RPC messages are embedded inside Nostr ephemeral events. Services are addressed by **public key** (not URL), and relays serve as message buses.

**Key value proposition:** Decentralized, censorship-resistant MCP services with built-in identity (Nostr keys), encryption (NIP-44), and payments (Lightning).

## Protocol Overview

### Event Kinds

| Kind  | Type        | Purpose                           |
|-------|-------------|-----------------------------------|
| 25910 | Ephemeral   | All MCP messages (requests, responses, notifications) |
| 1059  | Regular     | NIP-59 gift wrap (persistent encrypted envelope) |
| 21059 | Ephemeral   | NIP-59 gift wrap (ephemeral, CEP-19) |
| 11316 | Replaceable | Server announcement               |
| 11317 | Replaceable | Tools list                        |
| 11318 | Replaceable | Resources list                    |
| 11319 | Replaceable | Resource templates list           |
| 11320 | Replaceable | Prompts list                      |

### Message Structure

- **Content field**: Stringified JSON-RPC 2.0 message (standard MCP)
- **Tags for addressing**: `["p", "<recipient-pubkey>"]` on requests
- **Tags for correlation**: `["e", "<request-event-id>"]` on responses
- **Single kind**: All MCP traffic uses kind 25910

### Message Flow

1. **Client -> Server**: Kind 25910, content = JSON-RPC request, tags = `[["p", "<server-pubkey>"]]`
2. **Server -> Client**: Kind 25910, content = JSON-RPC response, tags = `[["e", "<request-event-id>"], ["p", "<client-pubkey>"]]`

### Initialization (Optional)

Standard MCP handshake transported over kind 25910:

1. Client sends `initialize` request with `["p", serverPubkey]`
2. Server responds with `initialize` result (capabilities) with `["e", requestEventId]`
3. Client sends `notifications/initialized` with `["p", serverPubkey]`

Servers can operate **statelessly** (skip initialization), useful for public services.

### ID Correlation Trick

The server transport replaces the JSON-RPC `id` in incoming requests with the Nostr event ID for internal routing. When sending the response, it restores the original JSON-RPC `id`. This enables correlating responses to the correct Nostr event for the `e` tag.

## CEP Specifications

### CEP-4: Encryption Support (Final)

- Optional end-to-end encryption using NIP-44 v2 + NIP-59 gift wrap
- No "rumor" layer (simplified NIP-17)
- Flow: Sign inner kind 25910 event -> NIP-44 encrypt with ephemeral keypair -> Wrap in kind 1059
- Discovery: `["support_encryption"]` tag in server announcements or init responses
- Metadata: Only recipient pubkey leaks via `p` tag on gift wrap

### CEP-6: Public Server Announcements (Final)

Replaceable events for service discovery:

- **Kind 11316** (Server Announcement): Content = MCP `initialize` response. Tags: `name`, `about`, `picture`, `website`, `support_encryption`, `support_encryption_ephemeral`, `pmi`, `cap`
- **Kinds 11317-11320**: Content = JSON arrays of tools/resources/prompts (mirrors MCP list responses)

These are replaceable events (10000-20000 range): relays store only the latest per pubkey+kind.

**How announcements are generated**: The server transport "self-sends" initialize + list requests internally, then publishes the responses as the appropriate announcement event kinds.

### CEP-8: Capability Pricing and Payment Flow (Draft)

#### Pricing Tags

```
["cap", "tool:get_weather", "100", "sats"]          // Fixed price
["cap", "tool:analyze", "100-1000", "sats"]          // Price range
```

Capability identifiers: `tool:<name>`, `prompt:<name>`, `resource:<uri>`

#### Payment Method Identifiers (PMI)

W3C PMI format. Advertised via `["pmi", "bitcoin-lightning-bolt11"]` tags on both server and client events.

#### Payment Flow

1. Client sends `tools/call` request (kind 25910)
2. Server matches to a `PricedCapability`
3. Server may call `resolvePrice()` for dynamic pricing (can quote, reject, or waive)
4. Server sends `notifications/payment_required`:
   ```json
   {
     "method": "notifications/payment_required",
     "params": { "amount": 100, "pay_req": "lnbc...", "pmi": "bitcoin-lightning-bolt11", "ttl": 600 }
   }
   ```
5. Client pays (e.g., via NWC)
6. Server verifies and sends `notifications/payment_accepted`
7. Server processes request and returns result

#### Direct Payment (Bearer Assets)

Optimization: client attaches `["direct_payment", "<pmi>", "<payload>"]` tag on request event. Server validates inline. Returns `["change", "<pmi>", "<remainder>"]` if overpaid.

### CEP-16: Client Public Key Injection (Final, Informational)

Server transport injects `_meta.clientPubkey` into incoming MCP requests. Enables per-client auth, rate limiting, personalization without extra protocol overhead.

### CEP-19: Ephemeral Gift Wraps (Draft)

Kind 21059 (ephemeral range) as alternative to kind 1059. Same structure but relays don't persist. Discovery via `["support_encryption_ephemeral"]` tag. Prefer 21059 when both peers support it.

### CEP-21: PMI Recommendations (Draft, Informational)

- Recommended PMI: `bitcoin-lightning-bolt11` (pay_req = BOLT11 invoice string)
- Naming convention: `<asset/network>-<payload-format>[-<sub-variant>]`
- `-direct` suffix for bearer-asset PMIs

## SDK Architecture (TypeScript Reference)

### Module Structure

```
@contextvm/sdk
  /core        - Constants, interfaces, encryption, serializers
  /transport   - NostrClientTransport, NostrServerTransport, BaseNostrTransport
  /relay       - ApplesauceRelayPool
  /signer      - PrivateKeySigner (NostrSigner interface)
  /gateway     - NostrMCPGateway (bridges existing MCP servers to Nostr)
  /proxy       - NostrMCPProxy (bridges local MCP clients to Nostr servers)
  /payments    - CEP-8 payment handlers, processors, middleware
```

### Key Interfaces

```typescript
// Signer abstraction
interface NostrSigner {
  getPublicKey(): Promise<string>;
  signEvent(event: EventTemplate): Promise<NostrEvent>;
  nip44?: { encrypt(pubkey, plaintext): string; decrypt(pubkey, ciphertext): string };
}

// Relay abstraction
interface RelayHandler {
  connect(): Promise<void>;
  disconnect(relayUrls?: string[]): Promise<void>;
  publish(event: NostrEvent, opts?): Promise<void>;
  subscribe(filters: Filter[], onEvent, onEose?): Promise<() => void>;
  unsubscribe(): void;
}
```

### Server Transport Configuration

```typescript
new NostrServerTransport({
  signer: serverSigner,
  relayHandler: ['wss://relay.contextvm.org'],
  serverInfo: { name: 'My Server', about: '...' },
  isPublicServer: true,                    // publish announcements
  allowedPublicKeys: ['<hex>'],            // optional whitelist
  excludedCapabilities: [...],             // bypass whitelist for specific caps
  encryptionMode: EncryptionMode.OPTIONAL, // optional | required | disabled
  giftWrapMode: GiftWrapMode.OPTIONAL,     // optional | ephemeral | persistent
  injectClientPubkey: true,                // CEP-16
  maxSessions: 1000,
  inboundMiddleware: async (msg, ctx, forward) => { ... },
});
```

### Client Transport Configuration

```typescript
new NostrClientTransport({
  signer: mySigner,
  relayHandler: ['wss://relay.contextvm.org'],
  serverPubkey: '<hex-pubkey-of-server>',
  encryptionMode: EncryptionMode.OPTIONAL,
  isStateless: false,  // true = skip initialization handshake
});
```

### Session Management

- Server maintains per-client sessions via `SessionStore` (LRU-bounded)
- `CorrelationStore` maps Nostr event IDs to client pubkeys + original JSON-RPC request IDs
- Progress notifications routed via progress token -> event ID mapping

### Payment Middleware

```typescript
// Server side
const paidTransport = withServerPayments(baseTransport, {
  processors: [new LnBolt11NwcPaymentProcessor(nwcClient)],
  pricedCapabilities: [
    { method: 'tools/call', name: 'transcode', amount: 100, currencyUnit: 'sats' }
  ],
  resolvePrice: async ({ capability, request, clientPubkey }) => {
    return quotePrice(100, { description: 'Video transcode' });
  },
});

// Client side
const paidTransport = withClientPayments(baseTransport, {
  handlers: [new LnBolt11NwcPaymentHandler(nwcClient)],
  paymentPolicy: (req, ctx) => req.amount <= 1000, // auto-approve under 1000 sats
});
```

## Rust Implementation Path

### No Rust SDK Exists

The only Rust example is `futurepaul/cvm-rust-mcp` which uses a **gateway approach**:
- Write MCP server in Rust using `rmcp` crate (v0.8) with stdio transport
- Use the ContextVM Gateway CLI (TypeScript/Deno) to bridge to Nostr

### Native Rust Implementation (Our Approach)

We would implement the ContextVM protocol directly in Rust. Key components needed:

#### 1. Message Serialization

```
Nostr Event (kind 25910):
  content = JSON.stringify(jsonRpcMessage)
  tags = [["p", recipientPubkey]] (for requests)
       or [["e", requestEventId], ["p", clientPubkey]] (for responses)
```

This is trivial — we already build Nostr events with `nostr-sdk`.

#### 2. Transport Layer

Subscribe to:
- Kind 25910 events tagged with our pubkey (`#p` filter)
- Kind 1059 events tagged with our pubkey (encrypted messages)
- Kind 21059 events tagged with our pubkey (ephemeral encrypted)

On receiving:
- If kind 1059/21059: decrypt gift wrap (NIP-44), extract inner kind 25910 event
- Parse `event.content` as JSON-RPC message
- Route based on method (initialize, tools/list, tools/call, etc.)

On sending:
- Build JSON-RPC response
- Create kind 25910 event with `["e", requestEventId]` and `["p", clientPubkey]`
- If encryption required: gift-wrap with NIP-44

#### 3. Server Announcements (CEP-6)

Publish replaceable events:
- Kind 11316: Server info + capabilities
- Kind 11317: Tools list
- Kind 11318-11320: Resources/prompts (if applicable)

Content = JSON matching MCP response format. Tags = metadata (`name`, `about`, etc.)

#### 4. MCP Protocol Layer

We need a minimal MCP server implementation:
- Handle `initialize` -> return capabilities
- Handle `tools/list` -> return tool definitions with JSON Schema
- Handle `tools/call` -> dispatch to our video processing pipeline
- Handle `notifications/initialized` -> acknowledge
- Send `notifications/progress` for long-running jobs

Could use the `rmcp` crate or implement the thin JSON-RPC layer ourselves (it's simple).

#### 5. Encryption (CEP-4)

We already use NIP-44 (`nostr-sdk` 0.35 has `nip44::encrypt`/`nip44::decrypt`). Gift wrapping:
1. Sign inner event (kind 25910)
2. Generate ephemeral keypair
3. NIP-44 encrypt serialized inner event with ephemeral secret key + recipient pubkey
4. Create kind 1059 event signed by ephemeral key, content = ciphertext, tags = `["p", recipientPubkey]`

#### 6. Payments (CEP-8, optional/future)

Payment notifications are just JSON-RPC notifications in the MCP stream. We'd need NWC (NIP-47) for Lightning invoice creation/verification.

## Integration Plan for nostube-transcode

### Phase 1: ContextVM Interface (Additive)

Add a new `cvm/` module alongside existing `dvm/`:

```
src/
  dvm/          # existing DVM protocol (kind 5207/6207)
    handler.rs
    events.rs
    encryption.rs
  cvm/          # new ContextVM protocol (kind 25910)
    transport.rs    # subscribe/publish kind 25910, gift wrap handling
    server.rs       # MCP server: initialize, tools/list, tools/call
    tools.rs        # tool definitions (transcode_video, get_status, etc.)
    announce.rs     # CEP-6 announcements (kinds 11316-11320)
    encryption.rs   # CEP-4 gift wrap (NIP-44 + NIP-59)
    admin.rs        # admin tools (get_config, set_relays, pause, etc.)
```

### Phase 2: Tool Definitions

Map our existing capabilities to MCP tools:

```json
{
  "name": "transcode_video",
  "description": "Transform a video into HLS format and upload to Blossom servers",
  "inputSchema": {
    "type": "object",
    "properties": {
      "video_url": { "type": "string", "description": "URL of video to transcode" },
      "resolutions": { "type": "array", "items": { "type": "string" }, "default": ["360p", "720p", "1080p"] }
    },
    "required": ["video_url"]
  }
}
```

Admin tools (protected by `clientPubkey` check via CEP-16):
- `get_config` / `set_relays` / `set_blossom_servers` / `set_profile`
- `pause` / `resume` / `status` / `job_history`

### Phase 3: Shared Core

Both DVM and CVM interfaces call the same core:
- `VideoProcessor` for transcoding
- `BlossomClient` for uploads
- `DvmState` for runtime state
- Config management

### Phase 4: Remove DVM (Later)

Once CVM interface is proven, remove the `dvm/` module and `nostr/` client/publisher (or repurpose for CVM transport).

## Key Relays

- `wss://relay.contextvm.org` (primary)
- `wss://relay2.contextvm.org`
- `wss://cvm.otherstuff.ai`

The ContextVM relay (`contextvm-relay`, written in Go) is purpose-built:
- LMDB for persistent storage (replaceable/addressable events)
- Circular buffer (500 events) for ephemeral events
- Only accepts ephemeral, gift wrap, replaceable, and addressable events
- Heartbeat system: pings servers with kind 11316, purges after N failures

## Dependencies for Rust Implementation

Already in use:
- `nostr-sdk` 0.35 — Nostr protocol, NIP-44, event building
- `tokio` — async runtime
- `serde` / `serde_json` — JSON serialization

May need:
- `rmcp` 0.8 — Rust MCP SDK (optional, could hand-roll the thin JSON-RPC layer)
- `schemars` 1.0 — JSON Schema generation for tool parameters (if using `rmcp`)
- `jsonrpc-core` or similar — if not using `rmcp`

## Ecosystem Projects (Reference)

- **Relatr** — Trust computation service (https://relatr.xyz)
- **Blovm** — File storage daemon with Blossom + MCP
- **Nutoff** — Cashu wallet with NWC
- **Wotrlay** — WoT-based relay
- **CVM Rust MCP Example** — https://github.com/futurepaul/cvm-rust-mcp

## Reference Links

- GitHub org: https://github.com/ContextVM
- SDK repo: https://github.com/ContextVM/sdk
- Docs repo: https://github.com/ContextVM/contextvm-docs
- Docs site: https://contextvm.github.io/contextvm-docs/
- Site: https://contextvm.github.io/contextvm-site/
- npm: `@contextvm/sdk`
- Gateway CLI: `jsr:@contextvm/gateway-cli`
