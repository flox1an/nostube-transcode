import { useState, useEffect } from "react";
import { discoverDvms, formatPubkey, type DvmService } from "../nostr/discovery";
import "./DvmSelector.css";

interface DvmSelectorProps {
  onSelect: (dvm: DvmService) => void;
  selectedDvm: DvmService | null;
  disabled?: boolean;
}

export function DvmSelector({ onSelect, selectedDvm, disabled }: DvmSelectorProps) {
  const [dvms, setDvms] = useState<DvmService[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;

    async function fetchDvms() {
      try {
        setLoading(true);
        setError(null);
        const discovered = await discoverDvms(5000);

        if (!mounted) return;

        setDvms(discovered);

        // Auto-select first DVM if none selected
        if (discovered.length > 0 && !selectedDvm) {
          onSelect(discovered[0]);
        }
      } catch (err) {
        if (!mounted) return;
        setError(err instanceof Error ? err.message : "Failed to discover DVMs");
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    }

    fetchDvms();

    return () => {
      mounted = false;
    };
  }, [onSelect, selectedDvm]);

  const handleRefresh = async () => {
    setLoading(true);
    setError(null);
    try {
      const discovered = await discoverDvms(5000);
      setDvms(discovered);
      if (discovered.length > 0 && !selectedDvm) {
        onSelect(discovered[0]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to discover DVMs");
    } finally {
      setLoading(false);
    }
  };

  const formatLastSeen = (timestamp: number) => {
    const diff = Math.floor(Date.now() / 1000 - timestamp);
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  };

  if (loading && dvms.length === 0) {
    return (
      <div className="dvm-selector">
        <div className="dvm-selector-header">
          <h3>Select DVM</h3>
        </div>
        <div className="dvm-loading">
          <div className="spinner" />
          <span>Discovering DVMs...</span>
        </div>
      </div>
    );
  }

  if (error && dvms.length === 0) {
    return (
      <div className="dvm-selector">
        <div className="dvm-selector-header">
          <h3>Select DVM</h3>
          <button className="refresh-btn" onClick={handleRefresh} disabled={loading}>
            Refresh
          </button>
        </div>
        <div className="dvm-error">
          <p>{error}</p>
          <p className="dvm-hint">No video transform DVMs found on the network.</p>
        </div>
      </div>
    );
  }

  if (dvms.length === 0) {
    return (
      <div className="dvm-selector">
        <div className="dvm-selector-header">
          <h3>Select DVM</h3>
          <button className="refresh-btn" onClick={handleRefresh} disabled={loading}>
            Refresh
          </button>
        </div>
        <div className="dvm-empty">
          <p>No video transform DVMs found.</p>
          <p className="dvm-hint">Make sure a DVM is running and announcing itself.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="dvm-selector">
      <div className="dvm-selector-header">
        <h3>Select DVM</h3>
        <button className="refresh-btn" onClick={handleRefresh} disabled={loading}>
          {loading ? "..." : "Refresh"}
        </button>
      </div>

      <div className="dvm-list">
        {dvms.map((dvm) => (
          <div
            key={dvm.pubkey}
            className={`dvm-item ${selectedDvm?.pubkey === dvm.pubkey ? "selected" : ""} ${disabled ? "disabled" : ""}`}
            onClick={() => !disabled && onSelect(dvm)}
          >
            <div className="dvm-item-header">
              <span className="dvm-name">{dvm.name}</span>
              <span className="dvm-pubkey">{formatPubkey(dvm.pubkey)}</span>
            </div>
            <div className="dvm-item-about">{dvm.about}</div>
            <div className="dvm-item-meta">
              <span className="dvm-modes">
                Modes: {dvm.supportedModes.join(", ")}
              </span>
              <span className="dvm-last-seen">
                Last seen: {formatLastSeen(dvm.lastSeen)}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
