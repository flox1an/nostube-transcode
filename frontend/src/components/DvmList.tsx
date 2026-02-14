// frontend/src/components/DvmList.tsx
import { useState, useEffect, useRef } from "react";
import type { Event } from "nostr-tools";
import { discoverDvms } from "../nostr/discovery";
import {
  queryOperatorDvms,
  parseDvmAnnouncement,
  sendAdminCommand,
  subscribeToAdminResponses,
  type DvmStatus,
} from "../nostr/admin";
import { getCurrentSigner } from "../nostr/client";
import { RELAYS } from "../nostr/constants";
import { UserAvatar } from "./UserAvatar";
import { UserName } from "./UserName";
import "./DvmList.css";

export type DvmFilter = "all" | "mine";

export interface UnifiedDvm {
  pubkey: string;
  name: string;
  about: string;
  relays: string[];
  isOwned: boolean;
  status?: DvmStatus;
  statusLoading?: boolean;
  statusError?: string;
  // For public DVMs
  supportedModes?: string[];
  supportedResolutions?: string[];
  lastSeen?: number;
  operatorPubkey?: string;
}

interface DvmListProps {
  userPubkey: string;
  selectedDvm: UnifiedDvm | null;
  onSelect: (dvm: UnifiedDvm) => void;
  onPairNew: () => void;
}

export function DvmList({ userPubkey, selectedDvm, onSelect, onPairNew }: DvmListProps) {
  const [filter, setFilter] = useState<DvmFilter>("mine");
  const [allDvms, setAllDvms] = useState<UnifiedDvm[]>([]);
  const [myDvms, setMyDvms] = useState<Map<string, UnifiedDvm>>(new Map());
  const [loading, setLoading] = useState(true);
  const queriedStatusRef = useRef<Set<string>>(new Set());

  // Discover all public DVMs
  useEffect(() => {
    let mounted = true;

    async function fetchPublicDvms() {
      try {
        const discovered = await discoverDvms(5000);
        if (!mounted) return;

        const unified: UnifiedDvm[] = discovered.map((d) => ({
          pubkey: d.pubkey,
          name: d.name,
          about: d.about,
          relays: d.relays,
          isOwned: false,
          supportedModes: d.supportedModes,
          supportedResolutions: d.supportedResolutions,
          lastSeen: d.lastSeen,
          operatorPubkey: d.operatorPubkey,
        }));

        setAllDvms(unified);
      } catch (err) {
        console.error("Failed to discover DVMs:", err);
      } finally {
        if (mounted) setLoading(false);
      }
    }

    fetchPublicDvms();
    return () => { mounted = false; };
  }, []);

  // Query DVMs owned by user
  useEffect(() => {
    queriedStatusRef.current.clear();

    const unsubscribe = queryOperatorDvms(
      userPubkey,
      RELAYS,
      (event: Event) => {
        const metadata = parseDvmAnnouncement(event);
        if (!metadata) return;

        if (queriedStatusRef.current.has(metadata.pubkey)) return;
        queriedStatusRef.current.add(metadata.pubkey);

        const dvm: UnifiedDvm = {
          pubkey: metadata.pubkey,
          name: metadata.name,
          about: metadata.about,
          relays: RELAYS,
          isOwned: true,
          statusLoading: true,
          operatorPubkey: metadata.operatorPubkey,
        };

        setMyDvms((prev) => {
          const next = new Map(prev);
          next.set(dvm.pubkey, dvm);
          return next;
        });

        // Query status
        const signer = getCurrentSigner();
        if (!signer) return;

        const unsubscribeStatus = subscribeToAdminResponses(
          signer,
          userPubkey,
          metadata.pubkey,
          RELAYS,
          (response) => {
            if (response.error) {
              console.error("Admin command failed:", response.error);
              return;
            }

            const data = response.result as Record<string, unknown>;
            if (!data) return;

            if ("paused" in data && "jobs_active" in data) {
              const status = data as unknown as DvmStatus;
              setMyDvms((prev) => {
                const next = new Map(prev);
                const existing = next.get(metadata.pubkey);
                if (existing) {
                  next.set(metadata.pubkey, {
                    ...existing,
                    status,
                    statusLoading: false,
                  });
                }
                return next;
              });
              unsubscribeStatus();
            }
          }
        );

        sendAdminCommand(signer, metadata.pubkey, "status", {}, RELAYS).catch((err) => {
          console.error("Failed to fetch status:", err);
          setMyDvms((prev) => {
            const next = new Map(prev);
            const existing = next.get(metadata.pubkey);
            if (existing) {
              next.set(metadata.pubkey, {
                ...existing,
                statusLoading: false,
                statusError: "Failed to fetch status",
              });
            }
            return next;
          });
          unsubscribeStatus();
        });

        // Timeout
        setTimeout(() => {
          setMyDvms((prev) => {
            const next = new Map(prev);
            const existing = next.get(metadata.pubkey);
            if (existing && existing.statusLoading) {
              next.set(metadata.pubkey, {
                ...existing,
                statusLoading: false,
                statusError: "Status request timed out",
              });
            }
            return next;
          });
          unsubscribeStatus();
        }, 5000);
      }
    );

    return () => unsubscribe();
  }, [userPubkey]);

  // Merge owned DVMs into all DVMs list
  const displayDvms = filter === "mine"
    ? Array.from(myDvms.values())
    : allDvms.map((d) => {
        const owned = myDvms.get(d.pubkey);
        return owned ? { ...d, ...owned, isOwned: true } : d;
      });

  const formatLastSeen = (timestamp?: number) => {
    if (!timestamp) return "";
    const diff = Math.floor(Date.now() / 1000 - timestamp);
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  };

  return (
    <div className="dvm-list">
      <div className="dvm-list-header">
        <div className="filter-toggle">
          <button
            className={filter === "mine" ? "active" : ""}
            onClick={() => setFilter("mine")}
          >
            My DVMs
          </button>
          <button
            className={filter === "all" ? "active" : ""}
            onClick={() => setFilter("all")}
          >
            All DVMs
          </button>
        </div>
        {filter === "mine" && (
          <button className="pair-button" onClick={onPairNew}>
            + Pair
          </button>
        )}
      </div>

      <div className="dvm-list-content">
        {loading && displayDvms.length === 0 && (
          <div className="dvm-list-loading">
            <div className="spinner" />
            <span>Discovering DVMs...</span>
          </div>
        )}

        {!loading && displayDvms.length === 0 && (
          <div className="dvm-list-empty">
            {filter === "mine" ? (
              <>
                <p>No DVMs paired yet.</p>
                <button onClick={onPairNew}>Pair a DVM</button>
              </>
            ) : (
              <p>No DVMs found on the network.</p>
            )}
          </div>
        )}

        {displayDvms.map((dvm) => (
          <div
            key={dvm.pubkey}
            className={`dvm-list-item ${selectedDvm?.pubkey === dvm.pubkey ? "selected" : ""} ${dvm.isOwned ? "owned" : ""}`}
            onClick={() => onSelect(dvm)}
          >
            <div className="dvm-item-main">
              <UserAvatar pubkey={dvm.pubkey} size={32} className="dvm-avatar" />
              <div className="dvm-item-details">
                <div className="dvm-item-header">
                  <span className="dvm-name">
                    {dvm.isOwned && <span className="manage-badge">Admin</span>}
                    {dvm.name || <UserName pubkey={dvm.pubkey} />}
                  </span>
                  {dvm.isOwned && (
                    <span className={`status-indicator ${dvm.status?.paused ? "paused" : "active"}`}>
                    </span>
                  )}
                </div>
                <div className="dvm-item-about">{dvm.about || "No description"}</div>
                <div className="dvm-item-meta">
                  <span className="dvm-pubkey"><UserName pubkey={dvm.pubkey} /></span>
                  {dvm.lastSeen && (
                    <span className="dvm-last-seen">{formatLastSeen(dvm.lastSeen)}</span>
                  )}
                </div>
              </div>
            </div>
            {dvm.isOwned && dvm.status && (
              <div className="dvm-item-stats">
                <span>{dvm.status.jobs_active} active</span>
                <span>•</span>
                <span>{dvm.status.jobs_completed} done</span>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
