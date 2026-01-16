import { useState, useEffect } from "react";
import type { Event } from "nostr-tools";
import type { ISigner } from "applesauce-signers";
import { isNip04Encrypted } from "../nostr/events";

interface EventDisplayProps {
  event: Event;
  title?: string;
  /** Signer for decryption (optional - if not provided, encrypted content shows as-is) */
  signer?: ISigner | null;
  /** DVM pubkey for decryption (required for decryption) */
  dvmPubkey?: string;
}

/** Check if an event has the encrypted tag */
function hasEncryptedTag(event: Event): boolean {
  return event.tags.some((t) => t[0] === "encrypted");
}

export function EventDisplay({ event, title = "DVM Request Event", signer, dvmPubkey }: EventDisplayProps) {
  const [expanded, setExpanded] = useState(false);
  const [decryptedContent, setDecryptedContent] = useState<string | null>(null);
  const [decryptError, setDecryptError] = useState<string | null>(null);

  const isEncrypted = hasEncryptedTag(event) && isNip04Encrypted(event.content);

  // Attempt to decrypt encrypted content when expanded
  useEffect(() => {
    if (!expanded || !isEncrypted || !signer?.nip04 || !dvmPubkey) {
      return;
    }

    const decrypt = async () => {
      try {
        const decrypted = await signer!.nip04!.decrypt(dvmPubkey, event.content);
        // Try to pretty-print if it's JSON
        try {
          const parsed = JSON.parse(decrypted);
          setDecryptedContent(JSON.stringify(parsed, null, 2));
        } catch {
          setDecryptedContent(decrypted);
        }
        setDecryptError(null);
      } catch (e) {
        setDecryptError(e instanceof Error ? e.message : "Decryption failed");
        setDecryptedContent(null);
      }
    };

    decrypt();
  }, [expanded, isEncrypted, signer, dvmPubkey, event.content]);

  // Create a display version of the event with decrypted content
  const getDisplayEvent = () => {
    if (!isEncrypted || !decryptedContent) {
      return event;
    }

    // Try to parse the decrypted content
    try {
      const parsed = JSON.parse(decryptedContent);
      return {
        ...event,
        content: parsed, // Show parsed content inline
        _encrypted_original: event.content, // Keep original for reference
      };
    } catch {
      return {
        ...event,
        content: decryptedContent,
        _encrypted_original: event.content,
      };
    }
  };

  const displayEvent = getDisplayEvent();
  const formattedJson = JSON.stringify(displayEvent, null, 2);

  return (
    <div className="event-display">
      <button
        className="event-toggle"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        {expanded ? "▼" : "▶"} {title}
        {isEncrypted && <span className="encrypted-badge">🔐 Encrypted</span>}
      </button>
      {expanded && (
        <>
          {isEncrypted && !signer?.nip04 && (
            <p className="decrypt-note">Content is encrypted. Signer not available for decryption.</p>
          )}
          {decryptError && (
            <p className="decrypt-error">Decryption error: {decryptError}</p>
          )}
          {isEncrypted && decryptedContent && (
            <p className="decrypt-note">Showing decrypted content</p>
          )}
          <pre className="event-json">
            <code>{formattedJson}</code>
          </pre>
        </>
      )}
    </div>
  );
}
