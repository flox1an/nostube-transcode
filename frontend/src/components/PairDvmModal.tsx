// frontend/src/components/PairDvmModal.tsx
import { useState, useCallback, useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import { nip19 } from "nostr-tools";
import { useCurrentUser } from "../hooks/useCurrentUser";
import { sendAdminCommand, subscribeToAdminResponses } from "../nostr/admin";
import { getCurrentSigner } from "../nostr/client";
import { RELAYS } from "../nostr/constants";
import "./PairDvmModal.css";

type PairingState = "idle" | "pairing" | "success" | "error";

interface PairDvmModalProps {
  onClose: () => void;
  onSuccess: () => void;
}

export function PairDvmModal({ onClose, onSuccess }: PairDvmModalProps) {
  const [searchParams] = useSearchParams();
  const { user } = useCurrentUser();

  const [dvmPubkey, setDvmPubkey] = useState("");
  const [secret, setSecret] = useState("");
  const [state, setState] = useState<PairingState>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Parse URL parameters
  useEffect(() => {
    const dvmParam = searchParams.get("dvm");
    const secretParam = searchParams.get("secret");

    if (dvmParam) {
      try {
        if (dvmParam.startsWith("npub")) {
          const decoded = nip19.decode(dvmParam);
          if (decoded.type === "npub") {
            setDvmPubkey(decoded.data);
          }
        } else {
          setDvmPubkey(dvmParam);
        }
      } catch (e) {
        console.error("Failed to parse DVM pubkey:", e);
      }
    }

    if (secretParam) {
      setSecret(secretParam);
    }
  }, [searchParams]);

  const handlePair = useCallback(async () => {
    if (!dvmPubkey || !secret) {
      setErrorMessage("Please enter both DVM pubkey and pairing secret");
      return;
    }

    if (!user) {
      setErrorMessage("Not logged in");
      return;
    }

    const signer = getCurrentSigner();
    if (!signer || !signer.nip04) {
      setErrorMessage("Signer does not support encryption (NIP-04 required)");
      return;
    }

    setState("pairing");
    setErrorMessage(null);

    try {
      const unsubscribe = subscribeToAdminResponses(
        signer,
        user.pubkey,
        dvmPubkey,
        RELAYS,
        (response) => {
          if (response.ok) {
            setState("success");
            unsubscribe();
            setTimeout(() => {
              onSuccess();
            }, 1500);
          } else {
            setState("error");
            setErrorMessage(response.error || "Pairing failed");
            unsubscribe();
          }
        }
      );

      await sendAdminCommand(
        signer,
        dvmPubkey,
        { cmd: "claim_admin", secret },
        RELAYS
      );

      setTimeout(() => {
        if (state === "pairing") {
          setState("error");
          setErrorMessage("No response from DVM. Check that it's running and the secret is correct.");
          unsubscribe();
        }
      }, 10000);
    } catch (err) {
      setState("error");
      setErrorMessage(err instanceof Error ? err.message : "Failed to send pairing request");
    }
  }, [dvmPubkey, secret, user, state, onSuccess]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="pair-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Pair DVM</h2>
          <button className="close-btn" onClick={onClose}>&times;</button>
        </div>

        <div className="modal-content">
          <p className="description">
            Enter the DVM pubkey and pairing secret from the DVM console to claim admin access.
          </p>

          <div className="form-group">
            <label htmlFor="dvm-pubkey">DVM Pubkey (npub or hex)</label>
            <input
              id="dvm-pubkey"
              type="text"
              value={dvmPubkey}
              onChange={(e) => setDvmPubkey(e.target.value)}
              placeholder="npub1... or hex pubkey"
              disabled={state === "pairing" || state === "success"}
            />
          </div>

          <div className="form-group">
            <label htmlFor="secret">Pairing Secret</label>
            <input
              id="secret"
              type="text"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              placeholder="xxxx-xxxx-xxxx"
              disabled={state === "pairing" || state === "success"}
            />
          </div>

          {errorMessage && <p className="error-message">{errorMessage}</p>}
          {state === "success" && <p className="success-message">Successfully paired! Refreshing...</p>}

          <div className="modal-actions">
            <button
              className="pair-btn"
              onClick={handlePair}
              disabled={state === "pairing" || state === "success" || !dvmPubkey || !secret}
            >
              {state === "pairing" ? "Pairing..." : "Pair DVM"}
            </button>
            <button
              className="cancel-btn"
              onClick={onClose}
              disabled={state === "pairing"}
            >
              Cancel
            </button>
          </div>

          {state === "pairing" && (
            <div className="pairing-indicator">
              <div className="spinner"></div>
              <span>Sending pairing request...</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
