# Unified Dashboard Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Consolidate admin dashboard and video processor into a single page at `/` with DVM filter (All/My DVMs), admin-only features (self-test, transcoding, config editing).

**Architecture:** Single-page app at `/` with login required. DVM list with toggle filter between public DVMs and operator-owned DVMs. When viewing your own DVM, show admin features (stats, config, self-test, test transcode). Pairing flow inline via modal.

**Tech Stack:** React, React Router, existing nostr/admin.ts and nostr/discovery.ts modules

---

## Summary of Changes

1. Remove `/admin`, `/admin/pair`, `/admin/dvm/:pubkey` routes
2. Create unified `Dashboard` component at `/`
3. Add DVM filter toggle (All DVMs / My DVMs)
4. Merge DvmDetail functionality into the selected DVM panel
5. Move SelfTest to only show when viewing your own DVM
6. Add test transcode feature for admin-owned DVMs
7. Convert PairDvm to a modal dialog
8. Clean up unused files

---

## Task 1: Create Dashboard Shell Component

**Files:**
- Create: `frontend/src/Dashboard.tsx`
- Create: `frontend/src/Dashboard.css`

**Step 1: Create the Dashboard component with login gate and basic layout**

```tsx
// frontend/src/Dashboard.tsx
import { useState, useCallback } from "react";
import { useCurrentUser } from "./hooks/useCurrentUser";
import { LoginDialog } from "./components/LoginDialog";
import "./Dashboard.css";

export function Dashboard() {
  const { user, isLoggedIn, logout } = useCurrentUser();
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleLogout = useCallback(() => {
    logout();
  }, [logout]);

  // Not logged in - show login screen
  if (!isLoggedIn || !user) {
    return (
      <div className="dashboard">
        <div className="login-screen">
          <h1>DVM Video Processor</h1>
          <p>Login to discover and manage DVMs</p>
          <LoginDialog onLogin={() => setErrorMessage(null)} onError={setErrorMessage} />
          {errorMessage && <p className="error-message">{errorMessage}</p>}
        </div>
      </div>
    );
  }

  const truncatePubkey = (pubkey: string): string => {
    return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
  };

  return (
    <div className="dashboard">
      <header className="dashboard-header">
        <h1>DVM Video Processor</h1>
        <div className="user-info">
          <span className="pubkey">{truncatePubkey(user.pubkey)}</span>
          <button className="logout-button" onClick={handleLogout}>
            Logout
          </button>
        </div>
      </header>

      <main className="dashboard-main">
        <div className="dashboard-content">
          <aside className="dvm-sidebar">
            {/* DVM list with filter will go here */}
            <p>DVM list placeholder</p>
          </aside>
          <section className="dvm-detail-panel">
            {/* Selected DVM detail will go here */}
            <p>Select a DVM to view details</p>
          </section>
        </div>
      </main>
    </div>
  );
}
```

**Step 2: Create the Dashboard CSS**

```css
/* frontend/src/Dashboard.css */
.dashboard {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background: #0a0a0a;
}

/* Login Screen */
.login-screen {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1.5rem;
  text-align: center;
  padding: 2rem;
}

.login-screen h1 {
  margin: 0;
  font-size: 2rem;
}

.login-screen p {
  margin: 0;
  color: #888;
}

/* Header */
.dashboard-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 2rem;
  border-bottom: 1px solid #333;
  background: #1a1a1a;
}

.dashboard-header h1 {
  margin: 0;
  font-size: 1.25rem;
}

.user-info {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.pubkey {
  font-family: monospace;
  font-size: 0.875rem;
  color: #888;
  background: #0d0d0d;
  padding: 0.375rem 0.75rem;
  border-radius: 4px;
}

.logout-button {
  padding: 0.5rem 1rem;
  font-size: 0.875rem;
  background: transparent;
  color: #888;
  border: 1px solid #444;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.logout-button:hover {
  color: white;
  border-color: #666;
}

/* Main Content */
.dashboard-main {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.dashboard-content {
  display: flex;
  flex: 1;
  max-width: 1400px;
  margin: 0 auto;
  width: 100%;
}

/* Sidebar */
.dvm-sidebar {
  width: 360px;
  min-width: 360px;
  border-right: 1px solid #333;
  display: flex;
  flex-direction: column;
  background: #111;
}

/* Detail Panel */
.dvm-detail-panel {
  flex: 1;
  padding: 2rem;
  overflow-y: auto;
}

/* Error message */
.error-message {
  color: #f87171;
  background: rgba(248, 113, 113, 0.1);
  border: 1px solid rgba(248, 113, 113, 0.3);
  padding: 0.75rem 1rem;
  border-radius: 6px;
  margin: 0;
}
```

**Step 3: Update App.tsx to use Dashboard**

```tsx
// frontend/src/App.tsx
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { Dashboard } from "./Dashboard";
import { PairDvm } from "./pages/PairDvm";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        {/* Keep pair route temporarily for deep links */}
        <Route path="/pair" element={<PairDvm />} />
        <Route path="/admin/pair" element={<Navigate to="/pair" replace />} />
        <Route path="/admin/*" element={<Navigate to="/" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
```

**Step 4: Verify the app builds**

Run: `cd frontend && npm run build`
Expected: Build succeeds with no errors

**Step 5: Commit**

```bash
git add frontend/src/Dashboard.tsx frontend/src/Dashboard.css frontend/src/App.tsx
git commit -m "$(cat <<'EOF'
feat: add Dashboard shell component

Create unified dashboard at / with login gate, replacing separate
admin and video processor pages. Basic layout with sidebar for
DVM list and main panel for details.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Create DvmList Component with Filter

**Files:**
- Create: `frontend/src/components/DvmList.tsx`
- Create: `frontend/src/components/DvmList.css`

**Step 1: Create the DvmList component**

```tsx
// frontend/src/components/DvmList.tsx
import { useState, useEffect, useCallback, useRef } from "react";
import type { Event } from "nostr-tools";
import { nip19 } from "nostr-tools";
import { discoverDvms, type DvmService } from "../nostr/discovery";
import {
  queryOperatorDvms,
  parseDvmAnnouncement,
  sendAdminCommand,
  subscribeToAdminResponses,
  type DvmMetadata,
  type DvmStatus,
} from "../nostr/admin";
import { getCurrentSigner } from "../nostr/client";
import { RELAYS } from "../nostr/constants";
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
}

interface DvmListProps {
  userPubkey: string;
  selectedDvm: UnifiedDvm | null;
  onSelect: (dvm: UnifiedDvm) => void;
  onPairNew: () => void;
}

export function DvmList({ userPubkey, selectedDvm, onSelect, onPairNew }: DvmListProps) {
  const [filter, setFilter] = useState<DvmFilter>("all");
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
            if ("paused" in response && "jobs_active" in response) {
              const status = response as unknown as DvmStatus;
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

        sendAdminCommand(signer, metadata.pubkey, { cmd: "status" }, RELAYS).catch((err) => {
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

  const formatPubkey = (pubkey: string): string => {
    try {
      const npub = nip19.npubEncode(pubkey);
      return `${npub.slice(0, 12)}...${npub.slice(-8)}`;
    } catch {
      return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
    }
  };

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
            className={filter === "all" ? "active" : ""}
            onClick={() => setFilter("all")}
          >
            All DVMs
          </button>
          <button
            className={filter === "mine" ? "active" : ""}
            onClick={() => setFilter("mine")}
          >
            My DVMs
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
            <div className="dvm-item-header">
              <span className="dvm-name">{dvm.name}</span>
              {dvm.isOwned && (
                <span className={`status-badge ${dvm.status?.paused ? "paused" : "active"}`}>
                  {dvm.statusLoading ? "..." : dvm.status?.paused ? "Paused" : "Active"}
                </span>
              )}
            </div>
            <div className="dvm-item-about">{dvm.about || "No description"}</div>
            <div className="dvm-item-meta">
              <span className="dvm-pubkey">{formatPubkey(dvm.pubkey)}</span>
              {dvm.lastSeen && (
                <span className="dvm-last-seen">{formatLastSeen(dvm.lastSeen)}</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

**Step 2: Create DvmList CSS**

```css
/* frontend/src/components/DvmList.css */
.dvm-list {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.dvm-list-header {
  padding: 1rem;
  border-bottom: 1px solid #333;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
}

.filter-toggle {
  display: flex;
  background: #0d0d0d;
  border: 1px solid #333;
  border-radius: 6px;
  overflow: hidden;
}

.filter-toggle button {
  padding: 0.5rem 0.875rem;
  font-size: 0.8125rem;
  font-weight: 500;
  background: transparent;
  color: #888;
  border: none;
  cursor: pointer;
  transition: all 0.2s;
}

.filter-toggle button:hover {
  color: white;
  background: rgba(255, 255, 255, 0.05);
}

.filter-toggle button.active {
  background: #6366f1;
  color: white;
}

.pair-button {
  padding: 0.5rem 0.75rem;
  font-size: 0.8125rem;
  font-weight: 500;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.2s;
}

.pair-button:hover {
  background: #4f46e5;
}

.dvm-list-content {
  flex: 1;
  overflow-y: auto;
}

.dvm-list-loading,
.dvm-list-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem;
  color: #888;
  text-align: center;
  gap: 1rem;
}

.dvm-list-empty button {
  padding: 0.5rem 1rem;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid #333;
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.dvm-list-item {
  padding: 1rem;
  border-bottom: 1px solid #222;
  cursor: pointer;
  transition: background 0.2s;
}

.dvm-list-item:hover {
  background: #1a1a1a;
}

.dvm-list-item.selected {
  background: #1a1a1a;
  border-left: 3px solid #6366f1;
}

.dvm-list-item.owned {
  border-left: 3px solid #34d399;
}

.dvm-list-item.owned.selected {
  border-left: 3px solid #6366f1;
}

.dvm-item-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.375rem;
}

.dvm-name {
  font-weight: 600;
  font-size: 0.9375rem;
}

.status-badge {
  padding: 0.125rem 0.5rem;
  border-radius: 10px;
  font-size: 0.6875rem;
  font-weight: 500;
  text-transform: uppercase;
}

.status-badge.active {
  background: rgba(52, 211, 153, 0.2);
  color: #34d399;
}

.status-badge.paused {
  background: rgba(251, 191, 36, 0.2);
  color: #fbbf24;
}

.dvm-item-about {
  font-size: 0.8125rem;
  color: #888;
  margin-bottom: 0.5rem;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.dvm-item-meta {
  display: flex;
  justify-content: space-between;
  font-size: 0.75rem;
  color: #666;
}

.dvm-pubkey {
  font-family: monospace;
}
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add frontend/src/components/DvmList.tsx frontend/src/components/DvmList.css
git commit -m "$(cat <<'EOF'
feat: add DvmList component with filter toggle

Unified DVM list that discovers public DVMs and queries user's
owned DVMs. Filter toggle switches between All DVMs and My DVMs.
Shows status badges for owned DVMs.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Create DvmDetailPanel Component

**Files:**
- Create: `frontend/src/components/DvmDetailPanel.tsx`
- Create: `frontend/src/components/DvmDetailPanel.css`

**Step 1: Create DvmDetailPanel component**

This component shows DVM details. For owned DVMs, it shows tabs: Overview, Config, Test Transcode, System/Self-Test.

```tsx
// frontend/src/components/DvmDetailPanel.tsx
import { useState, useEffect, useCallback, useRef } from "react";
import { nip19 } from "nostr-tools";
import type { Event } from "nostr-tools";
import type { UnifiedDvm } from "./DvmList";
import {
  sendAdminCommand,
  subscribeToAdminResponses,
  type DvmConfig,
  type DvmStatus,
  type DvmJob,
  type AdminResponse,
} from "../nostr/admin";
import { getCurrentSigner } from "../nostr/client";
import { RELAYS } from "../nostr/constants";
import { SelfTest } from "./SelfTest";
import { VideoForm, type OutputMode, type Resolution, type Codec, type HlsResolution } from "./VideoForm";
import { JobProgress, type StatusMessage } from "./JobProgress";
import { VideoPlayer } from "./VideoPlayer";
import { EventDisplay } from "./EventDisplay";
import { publishTransformRequest, subscribeToResponses } from "../nostr/client";
import { parseStatusEvent, parseResultEvent, type DvmResult } from "../nostr/events";
import "./DvmDetailPanel.css";

type TabType = "overview" | "config" | "transcode" | "system";

interface DvmDetailPanelProps {
  dvm: UnifiedDvm;
  userPubkey: string;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

export function DvmDetailPanel({ dvm, userPubkey }: DvmDetailPanelProps) {
  const [activeTab, setActiveTab] = useState<TabType>("overview");
  const [status, setStatus] = useState<DvmStatus | null>(dvm.status || null);
  const [config, setConfig] = useState<DvmConfig | null>(null);
  const [jobs, setJobs] = useState<DvmJob[]>([]);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  // Config editing
  const [editingConfig, setEditingConfig] = useState(false);
  const [configForm, setConfigForm] = useState<Partial<DvmConfig>>({});

  // Transcode state
  const [transcodeState, setTranscodeState] = useState<"idle" | "submitting" | "processing" | "complete" | "error">("idle");
  const [statusMessages, setStatusMessages] = useState<StatusMessage[]>([]);
  const [transcodeError, setTranscodeError] = useState<string | null>(null);
  const [dvmResult, setDvmResult] = useState<DvmResult | null>(null);
  const [requestEvent, setRequestEvent] = useState<Event | null>(null);
  const [responseEvent, setResponseEvent] = useState<Event | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);

  const subscriptionRef = useRef<(() => void) | null>(null);
  const initializedRef = useRef<string | null>(null);

  const handleAdminResponse = useCallback((response: AdminResponse) => {
    if (!response.ok) {
      console.error("Admin command failed:", response.error);
      return;
    }

    if ("paused" in response && "jobs_active" in response) {
      setStatus(response as unknown as DvmStatus);
    } else if ("config" in response) {
      const cfg = response.config as DvmConfig;
      setConfig(cfg);
      setConfigForm(cfg);
    } else if ("jobs" in response) {
      setJobs(response.jobs as DvmJob[]);
    }
  }, []);

  // Fetch admin data when viewing owned DVM
  useEffect(() => {
    if (!dvm.isOwned) return;
    if (initializedRef.current === dvm.pubkey) return;
    initializedRef.current = dvm.pubkey;

    const signer = getCurrentSigner();
    if (!signer) return;

    setLoading(true);

    const unsubscribe = subscribeToAdminResponses(
      signer,
      userPubkey,
      dvm.pubkey,
      RELAYS,
      handleAdminResponse
    );
    subscriptionRef.current = unsubscribe;

    Promise.all([
      sendAdminCommand(signer, dvm.pubkey, { cmd: "status" }, RELAYS),
      sendAdminCommand(signer, dvm.pubkey, { cmd: "get_config" }, RELAYS),
      sendAdminCommand(signer, dvm.pubkey, { cmd: "job_history", limit: 20 }, RELAYS),
    ])
      .then(() => setTimeout(() => setLoading(false), 1500))
      .catch((err) => {
        console.error("Failed to fetch DVM data:", err);
        setLoading(false);
      });

    return () => {
      if (subscriptionRef.current) {
        subscriptionRef.current();
        subscriptionRef.current = null;
      }
      initializedRef.current = null;
    };
  }, [dvm.pubkey, dvm.isOwned, userPubkey, handleAdminResponse]);

  const handlePauseResume = useCallback(async () => {
    const signer = getCurrentSigner();
    if (!signer || !status) return;

    setActionLoading(true);
    try {
      const cmd = status.paused ? { cmd: "resume" as const } : { cmd: "pause" as const };
      await sendAdminCommand(signer, dvm.pubkey, cmd, RELAYS);
      setTimeout(() => {
        sendAdminCommand(signer, dvm.pubkey, { cmd: "status" }, RELAYS);
        setActionLoading(false);
      }, 1000);
    } catch (err) {
      console.error("Failed to pause/resume:", err);
      setActionLoading(false);
    }
  }, [dvm.pubkey, status]);

  const handleSaveConfig = useCallback(async () => {
    const signer = getCurrentSigner();
    if (!signer || !configForm) return;

    setActionLoading(true);
    try {
      if (configForm.relays && configForm.relays.length > 0) {
        await sendAdminCommand(signer, dvm.pubkey, {
          cmd: "set_relays",
          relays: configForm.relays,
        }, RELAYS);
      }
      if (configForm.blossom_servers && configForm.blossom_servers.length > 0) {
        await sendAdminCommand(signer, dvm.pubkey, {
          cmd: "set_blossom_servers",
          servers: configForm.blossom_servers,
        }, RELAYS);
      }
      if (configForm.name && configForm.about !== undefined) {
        await sendAdminCommand(signer, dvm.pubkey, {
          cmd: "set_profile",
          name: configForm.name,
          about: configForm.about,
        }, RELAYS);
      }
      if (configForm.blob_expiration_days !== undefined) {
        await sendAdminCommand(signer, dvm.pubkey, {
          cmd: "set_blob_expiration",
          days: configForm.blob_expiration_days,
        }, RELAYS);
      }

      setEditingConfig(false);
      setTimeout(() => {
        sendAdminCommand(signer, dvm.pubkey, { cmd: "get_config" }, RELAYS);
        setActionLoading(false);
      }, 1000);
    } catch (err) {
      console.error("Failed to save config:", err);
      setActionLoading(false);
    }
  }, [dvm.pubkey, configForm]);

  // Transcode handlers
  const handleTranscodeSubmit = useCallback(async (
    videoUrl: string,
    mode: OutputMode,
    resolution: Resolution,
    codec: Codec,
    hlsResolutions?: HlsResolution[],
    encryption?: boolean
  ) => {
    setTranscodeState("submitting");
    setStatusMessages([]);
    setTranscodeError(null);
    setDvmResult(null);
    setRequestEvent(null);
    setResponseEvent(null);

    if (unsubscribeRef.current) {
      unsubscribeRef.current();
      unsubscribeRef.current = null;
    }

    try {
      const { eventId, signedEvent } = await publishTransformRequest(
        videoUrl,
        dvm.pubkey,
        dvm.relays,
        mode,
        resolution,
        codec,
        hlsResolutions,
        encryption
      );
      setRequestEvent(signedEvent);
      setTranscodeState("processing");

      const signer = getCurrentSigner();
      unsubscribeRef.current = subscribeToResponses(
        eventId,
        dvm.pubkey,
        dvm.relays,
        (response) => {
          if (response.type === "status") {
            parseStatusEvent(response.event, signer ?? undefined, dvm.pubkey)
              .then(({ status, message, eta }) => {
                setStatusMessages((prev) => [
                  ...prev,
                  { status, message, timestamp: Date.now(), eta },
                ]);
                if (status === "error") {
                  setTranscodeState("error");
                  setTranscodeError(message || "Job failed");
                }
              })
              .catch(console.error);
          } else if (response.type === "result") {
            setResponseEvent(response.event);
            parseResultEvent(response.event, signer ?? undefined, dvm.pubkey)
              .then((result) => {
                if (result) {
                  setDvmResult(result);
                  setTranscodeState("complete");
                }
              })
              .catch(console.error);
          }
        }
      );
    } catch (err) {
      setTranscodeState("error");
      setTranscodeError(err instanceof Error ? err.message : "Failed to submit request");
    }
  }, [dvm.pubkey, dvm.relays]);

  const handleTranscodeReset = useCallback(() => {
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
      unsubscribeRef.current = null;
    }
    setTranscodeState("idle");
    setStatusMessages([]);
    setTranscodeError(null);
    setDvmResult(null);
    setRequestEvent(null);
    setResponseEvent(null);
  }, []);

  const getNpub = (pubkey: string): string => {
    try {
      return nip19.npubEncode(pubkey);
    } catch {
      return pubkey;
    }
  };

  const formatUptime = (secs: number): string => {
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const minutes = Math.floor((secs % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  };

  const formatTimestamp = (ts: number): string => {
    return new Date(ts * 1000).toLocaleString();
  };

  // Public DVM view (not owned)
  if (!dvm.isOwned) {
    return (
      <div className="dvm-detail-panel">
        <div className="dvm-detail-header">
          <h2>{dvm.name}</h2>
          <code className="npub">{getNpub(dvm.pubkey)}</code>
        </div>
        <div className="public-dvm-info">
          <p className="dvm-about">{dvm.about || "No description"}</p>
          {dvm.supportedModes && (
            <p><strong>Modes:</strong> {dvm.supportedModes.join(", ")}</p>
          )}
          {dvm.supportedResolutions && (
            <p><strong>Resolutions:</strong> {dvm.supportedResolutions.join(", ")}</p>
          )}
          <p className="not-owned-notice">
            You don't operate this DVM. Switch to "My DVMs" to manage your own DVMs.
          </p>
        </div>
      </div>
    );
  }

  // Owned DVM view with tabs
  return (
    <div className="dvm-detail-panel">
      <div className="dvm-detail-header">
        <div className="header-left">
          <h2>{config?.name || dvm.name}</h2>
          <code className="npub">{getNpub(dvm.pubkey)}</code>
        </div>
        <div className="header-actions">
          {status && (
            <span className={`status-badge ${status.paused ? "paused" : "active"}`}>
              {status.paused ? "Paused" : "Active"}
            </span>
          )}
          <button
            className="action-btn"
            onClick={handlePauseResume}
            disabled={actionLoading || !status}
          >
            {actionLoading ? "..." : status?.paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      <div className="detail-tabs">
        <button className={activeTab === "overview" ? "active" : ""} onClick={() => setActiveTab("overview")}>
          Overview
        </button>
        <button className={activeTab === "config" ? "active" : ""} onClick={() => setActiveTab("config")}>
          Config
        </button>
        <button className={activeTab === "transcode" ? "active" : ""} onClick={() => setActiveTab("transcode")}>
          Test Transcode
        </button>
        <button className={activeTab === "system" ? "active" : ""} onClick={() => setActiveTab("system")}>
          System
        </button>
      </div>

      <div className="detail-content">
        {loading && (
          <div className="loading-state">
            <div className="spinner"></div>
            <p>Loading DVM data...</p>
          </div>
        )}

        {!loading && activeTab === "overview" && (
          <div className="overview-tab">
            {status && (
              <>
                <div className="stats-grid">
                  <div className="stat-card">
                    <h3>Uptime</h3>
                    <p className="stat-value">{formatUptime(status.uptime_secs)}</p>
                  </div>
                  <div className="stat-card">
                    <h3>Active Jobs</h3>
                    <p className="stat-value">{status.jobs_active}</p>
                  </div>
                  <div className="stat-card">
                    <h3>Completed</h3>
                    <p className="stat-value">{status.jobs_completed}</p>
                  </div>
                  <div className="stat-card">
                    <h3>Failed</h3>
                    <p className="stat-value">{status.jobs_failed}</p>
                  </div>
                </div>

                {status.hwaccel && (
                  <div className="info-section">
                    <h3>Hardware Acceleration</h3>
                    <p>{status.hwaccel}</p>
                  </div>
                )}

                <div className="info-section">
                  <h3>Version</h3>
                  <p>{status.version}</p>
                </div>
              </>
            )}

            {jobs.length > 0 && (
              <div className="recent-jobs">
                <h3>Recent Jobs</h3>
                <table className="jobs-table">
                  <thead>
                    <tr>
                      <th>Status</th>
                      <th>Input URL</th>
                      <th>Started</th>
                      <th>Duration</th>
                    </tr>
                  </thead>
                  <tbody>
                    {jobs.slice(0, 5).map((job) => (
                      <tr key={job.id}>
                        <td>
                          <span className={`job-status ${job.status}`}>{job.status}</span>
                        </td>
                        <td className="truncate">{job.input_url}</td>
                        <td>{formatTimestamp(job.started_at)}</td>
                        <td>{job.duration_secs ? `${job.duration_secs}s` : "-"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}

        {!loading && activeTab === "config" && config && (
          <div className="config-tab">
            {!editingConfig ? (
              <>
                <div className="config-section">
                  <h3>Profile</h3>
                  <p><strong>Name:</strong> {config.name}</p>
                  <p><strong>About:</strong> {config.about}</p>
                </div>

                <div className="config-section">
                  <h3>Relays</h3>
                  <ul className="list">
                    {config.relays.map((relay, i) => <li key={i}>{relay}</li>)}
                  </ul>
                </div>

                <div className="config-section">
                  <h3>Blossom Servers</h3>
                  <ul className="list">
                    {config.blossom_servers.map((server, i) => <li key={i}>{server}</li>)}
                  </ul>
                </div>

                <div className="config-section">
                  <h3>Blob Expiration</h3>
                  <p>{config.blob_expiration_days} days</p>
                </div>

                <button className="edit-btn" onClick={() => setEditingConfig(true)}>
                  Edit Configuration
                </button>
              </>
            ) : (
              <div className="config-form">
                <div className="form-group">
                  <label>Name</label>
                  <input
                    type="text"
                    value={configForm.name || ""}
                    onChange={(e) => setConfigForm({ ...configForm, name: e.target.value })}
                  />
                </div>

                <div className="form-group">
                  <label>About</label>
                  <textarea
                    value={configForm.about || ""}
                    onChange={(e) => setConfigForm({ ...configForm, about: e.target.value })}
                  />
                </div>

                <div className="form-group">
                  <label>Relays (one per line)</label>
                  <textarea
                    value={configForm.relays?.join("\n") || ""}
                    onChange={(e) =>
                      setConfigForm({
                        ...configForm,
                        relays: e.target.value.split("\n").filter((r) => r.trim()),
                      })
                    }
                  />
                </div>

                <div className="form-group">
                  <label>Blossom Servers (one per line)</label>
                  <textarea
                    value={configForm.blossom_servers?.join("\n") || ""}
                    onChange={(e) =>
                      setConfigForm({
                        ...configForm,
                        blossom_servers: e.target.value.split("\n").filter((s) => s.trim()),
                      })
                    }
                  />
                </div>

                <div className="form-group">
                  <label>Blob Expiration (days)</label>
                  <input
                    type="number"
                    value={configForm.blob_expiration_days || 30}
                    onChange={(e) =>
                      setConfigForm({
                        ...configForm,
                        blob_expiration_days: parseInt(e.target.value, 10),
                      })
                    }
                  />
                </div>

                <div className="form-actions">
                  <button className="save-btn" onClick={handleSaveConfig} disabled={actionLoading}>
                    {actionLoading ? "Saving..." : "Save Changes"}
                  </button>
                  <button
                    className="cancel-btn"
                    onClick={() => { setEditingConfig(false); setConfigForm(config); }}
                    disabled={actionLoading}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
          </div>
        )}

        {!loading && activeTab === "transcode" && (
          <div className="transcode-tab">
            <h3>Test Transcode</h3>
            <p className="tab-description">
              Submit a test video to this DVM to verify it's working correctly.
            </p>

            <VideoForm
              onSubmit={handleTranscodeSubmit}
              disabled={transcodeState === "submitting" || transcodeState === "processing"}
            />

            {requestEvent && (
              <EventDisplay
                event={requestEvent}
                signer={getCurrentSigner()}
                dvmPubkey={dvm.pubkey}
              />
            )}

            <JobProgress messages={statusMessages} error={transcodeError || undefined} />

            {dvmResult && dvmResult.type === "hls" && (
              <VideoPlayer src={dvmResult.master_playlist} encryptionKey={dvmResult.encryption_key} />
            )}

            {dvmResult && dvmResult.type === "mp4" && dvmResult.urls[0] && (
              <video className="mp4-player" src={dvmResult.urls[0]} controls playsInline />
            )}

            {responseEvent && (
              <EventDisplay
                event={responseEvent}
                title="DVM Response Event"
                signer={getCurrentSigner()}
                dvmPubkey={dvm.pubkey}
              />
            )}

            {dvmResult && (
              <div className="result-details">
                <h3>Result Details</h3>
                {dvmResult.type === "hls" && (
                  <div className="hls-details">
                    <p className="total-size">
                      <strong>Total Size:</strong> {formatBytes(dvmResult.total_size_bytes)}
                    </p>
                    <table className="stream-table">
                      <thead>
                        <tr>
                          <th>Resolution</th>
                          <th>Size</th>
                          <th>Playlist</th>
                        </tr>
                      </thead>
                      <tbody>
                        {dvmResult.stream_playlists.map((stream, i) => (
                          <tr key={i}>
                            <td>{stream.resolution}</td>
                            <td>{formatBytes(stream.size_bytes)}</td>
                            <td>
                              <a href={stream.url} target="_blank" rel="noopener noreferrer">View</a>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
                {dvmResult.type === "mp4" && (
                  <div className="mp4-details">
                    <p><strong>Resolution:</strong> {dvmResult.resolution}</p>
                    <p><strong>File Size:</strong> {formatBytes(dvmResult.size_bytes)}</p>
                    <p><strong>Servers:</strong> {dvmResult.urls.length}</p>
                  </div>
                )}
              </div>
            )}

            {(transcodeState === "complete" || transcodeState === "error") && (
              <button className="reset-button" onClick={handleTranscodeReset}>
                Test Another Video
              </button>
            )}
          </div>
        )}

        {!loading && activeTab === "system" && (
          <div className="system-tab">
            <SelfTest />
          </div>
        )}
      </div>
    </div>
  );
}
```

**Step 2: Create DvmDetailPanel CSS**

```css
/* frontend/src/components/DvmDetailPanel.css */
.dvm-detail-panel {
  max-width: 900px;
}

.dvm-detail-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 1.5rem;
  flex-wrap: wrap;
  gap: 1rem;
}

.header-left h2 {
  margin: 0 0 0.5rem 0;
  font-size: 1.5rem;
}

.npub {
  font-size: 0.75rem;
  color: #888;
  background: #1a1a1a;
  padding: 0.375rem 0.625rem;
  border-radius: 4px;
  word-break: break-all;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.status-badge {
  padding: 0.375rem 0.875rem;
  border-radius: 12px;
  font-size: 0.8125rem;
  font-weight: 500;
}

.status-badge.active {
  background: rgba(52, 211, 153, 0.2);
  color: #34d399;
}

.status-badge.paused {
  background: rgba(251, 191, 36, 0.2);
  color: #fbbf24;
}

.action-btn {
  padding: 0.5rem 1rem;
  font-size: 0.875rem;
  background: #333;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.2s;
}

.action-btn:hover:not(:disabled) {
  background: #444;
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Public DVM Info */
.public-dvm-info {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1.5rem;
}

.public-dvm-info .dvm-about {
  color: #ccc;
  margin-bottom: 1rem;
}

.public-dvm-info p {
  margin: 0.5rem 0;
  color: #888;
}

.not-owned-notice {
  margin-top: 1.5rem !important;
  padding-top: 1rem;
  border-top: 1px solid #333;
  color: #666 !important;
  font-style: italic;
}

/* Tabs */
.detail-tabs {
  display: flex;
  gap: 0.25rem;
  border-bottom: 1px solid #333;
  margin-bottom: 1.5rem;
}

.detail-tabs button {
  padding: 0.75rem 1.25rem;
  font-size: 0.875rem;
  font-weight: 500;
  background: transparent;
  color: #888;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s;
}

.detail-tabs button:hover {
  color: white;
}

.detail-tabs button.active {
  color: white;
  border-bottom-color: #6366f1;
}

/* Loading */
.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 3rem;
  color: #888;
  gap: 1rem;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid #333;
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Stats Grid */
.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.stat-card {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1rem;
}

.stat-card h3 {
  margin: 0 0 0.5rem 0;
  font-size: 0.75rem;
  color: #888;
  text-transform: uppercase;
}

.stat-card .stat-value {
  font-size: 1.5rem;
  font-weight: 600;
}

/* Info Section */
.info-section {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.info-section h3 {
  margin: 0 0 0.5rem 0;
  font-size: 0.875rem;
  color: #888;
}

.info-section p {
  margin: 0;
}

/* Recent Jobs */
.recent-jobs {
  margin-top: 1.5rem;
}

.recent-jobs h3 {
  margin: 0 0 1rem 0;
  font-size: 1rem;
}

.jobs-table {
  width: 100%;
  border-collapse: collapse;
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  overflow: hidden;
}

.jobs-table th,
.jobs-table td {
  padding: 0.75rem;
  text-align: left;
  border-bottom: 1px solid #333;
}

.jobs-table th {
  color: #888;
  font-weight: 500;
  font-size: 0.75rem;
  text-transform: uppercase;
  background: #111;
}

.jobs-table td {
  font-size: 0.875rem;
}

.jobs-table .truncate {
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.job-status {
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
  text-transform: uppercase;
}

.job-status.completed {
  background: rgba(52, 211, 153, 0.2);
  color: #34d399;
}

.job-status.failed {
  background: rgba(248, 113, 113, 0.2);
  color: #f87171;
}

.job-status.processing {
  background: rgba(99, 102, 241, 0.2);
  color: #a5b4fc;
}

/* Config Tab */
.config-section {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1.25rem;
  margin-bottom: 1rem;
}

.config-section h3 {
  margin: 0 0 0.75rem 0;
  font-size: 0.875rem;
  color: #888;
}

.config-section p {
  margin: 0.375rem 0;
}

.config-section .list {
  margin: 0;
  padding-left: 1.25rem;
}

.config-section .list li {
  margin: 0.25rem 0;
  font-family: monospace;
  font-size: 0.875rem;
  color: #ccc;
}

.edit-btn {
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.2s;
}

.edit-btn:hover {
  background: #4f46e5;
}

/* Config Form */
.config-form {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1.5rem;
}

.form-group {
  margin-bottom: 1.25rem;
}

.form-group label {
  display: block;
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
  color: #888;
}

.form-group input,
.form-group textarea {
  width: 100%;
  padding: 0.75rem;
  font-size: 0.9375rem;
  background: #0d0d0d;
  color: white;
  border: 1px solid #333;
  border-radius: 6px;
  box-sizing: border-box;
}

.form-group textarea {
  min-height: 100px;
  resize: vertical;
  font-family: monospace;
}

.form-group input:focus,
.form-group textarea:focus {
  outline: none;
  border-color: #6366f1;
}

.form-actions {
  display: flex;
  gap: 0.75rem;
  margin-top: 1.5rem;
}

.save-btn {
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.save-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.cancel-btn {
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  background: transparent;
  color: #888;
  border: 1px solid #444;
  border-radius: 6px;
  cursor: pointer;
}

.cancel-btn:hover:not(:disabled) {
  color: white;
  border-color: #666;
}

/* Transcode Tab */
.transcode-tab {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.transcode-tab h3 {
  margin: 0;
}

.tab-description {
  color: #888;
  margin: 0;
}

.reset-button {
  padding: 0.75rem 1.5rem;
  font-size: 1rem;
  background: transparent;
  color: white;
  border: 1px solid #444;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s;
  align-self: flex-start;
}

.reset-button:hover {
  border-color: #666;
  background: #1a1a1a;
}

.mp4-player {
  width: 100%;
  max-height: 400px;
  background: #000;
  border-radius: 8px;
}

.result-details {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1.5rem;
}

.result-details h3 {
  margin: 0 0 1rem 0;
}

.stream-table {
  width: 100%;
  border-collapse: collapse;
}

.stream-table th,
.stream-table td {
  padding: 0.75rem;
  text-align: left;
  border-bottom: 1px solid #333;
}

.stream-table th {
  color: #888;
  font-size: 0.75rem;
  text-transform: uppercase;
}

.stream-table a {
  color: #6366f1;
}

/* System Tab */
.system-tab {
  max-width: 600px;
}
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add frontend/src/components/DvmDetailPanel.tsx frontend/src/components/DvmDetailPanel.css
git commit -m "$(cat <<'EOF'
feat: add DvmDetailPanel with tabs for owned DVMs

Shows DVM info for public DVMs. For owned DVMs, provides tabs:
Overview (stats, recent jobs), Config (edit settings), Test
Transcode (submit test video), System (self-test).

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Wire Up Dashboard with DvmList and DvmDetailPanel

**Files:**
- Modify: `frontend/src/Dashboard.tsx`

**Step 1: Update Dashboard to use DvmList and DvmDetailPanel**

```tsx
// frontend/src/Dashboard.tsx
import { useState, useCallback } from "react";
import { useCurrentUser } from "./hooks/useCurrentUser";
import { LoginDialog } from "./components/LoginDialog";
import { DvmList, type UnifiedDvm } from "./components/DvmList";
import { DvmDetailPanel } from "./components/DvmDetailPanel";
import { PairDvmModal } from "./components/PairDvmModal";
import "./Dashboard.css";

export function Dashboard() {
  const { user, isLoggedIn, logout } = useCurrentUser();
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [selectedDvm, setSelectedDvm] = useState<UnifiedDvm | null>(null);
  const [showPairModal, setShowPairModal] = useState(false);

  const handleLogout = useCallback(() => {
    logout();
    setSelectedDvm(null);
  }, [logout]);

  const handleDvmSelect = useCallback((dvm: UnifiedDvm) => {
    setSelectedDvm(dvm);
  }, []);

  const handlePairNew = useCallback(() => {
    setShowPairModal(true);
  }, []);

  const handlePairComplete = useCallback(() => {
    setShowPairModal(false);
    // Refresh will happen naturally when the DVM list queries again
  }, []);

  // Not logged in - show login screen
  if (!isLoggedIn || !user) {
    return (
      <div className="dashboard">
        <div className="login-screen">
          <h1>DVM Video Processor</h1>
          <p>Login to discover and manage DVMs</p>
          <LoginDialog onLogin={() => setErrorMessage(null)} onError={setErrorMessage} />
          {errorMessage && <p className="error-message">{errorMessage}</p>}
        </div>
      </div>
    );
  }

  const truncatePubkey = (pubkey: string): string => {
    return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
  };

  return (
    <div className="dashboard">
      <header className="dashboard-header">
        <h1>DVM Video Processor</h1>
        <div className="user-info">
          <span className="pubkey">{truncatePubkey(user.pubkey)}</span>
          <button className="logout-button" onClick={handleLogout}>
            Logout
          </button>
        </div>
      </header>

      <main className="dashboard-main">
        <div className="dashboard-content">
          <aside className="dvm-sidebar">
            <DvmList
              userPubkey={user.pubkey}
              selectedDvm={selectedDvm}
              onSelect={handleDvmSelect}
              onPairNew={handlePairNew}
            />
          </aside>
          <section className="dvm-detail-panel-container">
            {selectedDvm ? (
              <DvmDetailPanel dvm={selectedDvm} userPubkey={user.pubkey} />
            ) : (
              <div className="no-selection">
                <p>Select a DVM from the list to view details</p>
              </div>
            )}
          </section>
        </div>
      </main>

      {showPairModal && (
        <PairDvmModal
          onClose={() => setShowPairModal(false)}
          onSuccess={handlePairComplete}
        />
      )}
    </div>
  );
}
```

**Step 2: Update Dashboard.css to add missing styles**

Add to the end of `frontend/src/Dashboard.css`:

```css
/* Add to end of Dashboard.css */

.dvm-detail-panel-container {
  flex: 1;
  padding: 2rem;
  overflow-y: auto;
}

.no-selection {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #666;
}

.no-selection p {
  margin: 0;
  font-size: 1rem;
}
```

**Step 3: Build will fail because PairDvmModal doesn't exist yet - that's Task 5**

---

## Task 5: Create PairDvmModal Component

**Files:**
- Create: `frontend/src/components/PairDvmModal.tsx`
- Create: `frontend/src/components/PairDvmModal.css`

**Step 1: Create PairDvmModal component (refactored from PairDvm page)**

```tsx
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
```

**Step 2: Create PairDvmModal CSS**

```css
/* frontend/src/components/PairDvmModal.css */
.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.75);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.pair-modal {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 12px;
  width: 100%;
  max-width: 480px;
  margin: 1rem;
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid #333;
}

.modal-header h2 {
  margin: 0;
  font-size: 1.25rem;
}

.close-btn {
  background: transparent;
  border: none;
  color: #888;
  font-size: 1.5rem;
  cursor: pointer;
  padding: 0;
  line-height: 1;
}

.close-btn:hover {
  color: white;
}

.modal-content {
  padding: 1.5rem;
}

.description {
  color: #888;
  margin: 0 0 1.5rem 0;
  font-size: 0.9375rem;
}

.form-group {
  margin-bottom: 1.25rem;
}

.form-group label {
  display: block;
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
  color: #888;
}

.form-group input {
  width: 100%;
  padding: 0.875rem 1rem;
  font-size: 0.9375rem;
  background: #0d0d0d;
  color: white;
  border: 1px solid #333;
  border-radius: 8px;
  box-sizing: border-box;
}

.form-group input:focus {
  outline: none;
  border-color: #6366f1;
}

.form-group input:disabled {
  opacity: 0.5;
}

.error-message {
  color: #f87171;
  background: rgba(248, 113, 113, 0.1);
  border: 1px solid rgba(248, 113, 113, 0.3);
  padding: 0.75rem 1rem;
  border-radius: 6px;
  margin: 0 0 1rem 0;
  font-size: 0.875rem;
}

.success-message {
  color: #34d399;
  background: rgba(52, 211, 153, 0.1);
  border: 1px solid rgba(52, 211, 153, 0.3);
  padding: 0.75rem 1rem;
  border-radius: 6px;
  margin: 0 0 1rem 0;
  font-size: 0.875rem;
}

.modal-actions {
  display: flex;
  gap: 0.75rem;
}

.pair-btn {
  flex: 1;
  padding: 0.875rem 1.5rem;
  font-size: 1rem;
  font-weight: 600;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.2s;
}

.pair-btn:hover:not(:disabled) {
  background: #4f46e5;
}

.pair-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.cancel-btn {
  padding: 0.875rem 1.5rem;
  font-size: 1rem;
  background: transparent;
  color: #888;
  border: 1px solid #444;
  border-radius: 8px;
  cursor: pointer;
}

.cancel-btn:hover:not(:disabled) {
  color: white;
  border-color: #666;
}

.pairing-indicator {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 1rem;
  color: #888;
  font-size: 0.875rem;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid #333;
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add frontend/src/components/PairDvmModal.tsx frontend/src/components/PairDvmModal.css frontend/src/Dashboard.tsx frontend/src/Dashboard.css
git commit -m "$(cat <<'EOF'
feat: add PairDvmModal and wire up Dashboard

Convert PairDvm page to a modal dialog. Dashboard now shows
DvmList in sidebar and DvmDetailPanel in main area. Pair modal
opens from My DVMs filter.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Update App.tsx Routes and Clean Up

**Files:**
- Modify: `frontend/src/App.tsx`
- Delete: `frontend/src/VideoProcessor.tsx`
- Delete: `frontend/src/VideoProcessor.css`
- Delete: `frontend/src/pages/AdminDashboard.tsx`
- Delete: `frontend/src/pages/AdminDashboard.css`
- Delete: `frontend/src/pages/DvmDetail.tsx`
- Delete: `frontend/src/pages/DvmDetail.css`
- Delete: `frontend/src/pages/PairDvm.tsx`
- Delete: `frontend/src/pages/PairDvm.css`
- Delete: `frontend/src/components/DvmSelector.tsx`
- Delete: `frontend/src/components/DvmSelector.css`

**Step 1: Update App.tsx with simplified routes**

```tsx
// frontend/src/App.tsx
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { Dashboard } from "./Dashboard";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        {/* Legacy redirects */}
        <Route path="/admin/*" element={<Navigate to="/" replace />} />
        <Route path="/pair" element={<Navigate to="/" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
```

**Step 2: Delete old files**

Run:
```bash
rm frontend/src/VideoProcessor.tsx frontend/src/VideoProcessor.css
rm frontend/src/pages/AdminDashboard.tsx frontend/src/pages/AdminDashboard.css
rm frontend/src/pages/DvmDetail.tsx frontend/src/pages/DvmDetail.css
rm frontend/src/pages/PairDvm.tsx frontend/src/pages/PairDvm.css
rm frontend/src/components/DvmSelector.tsx frontend/src/components/DvmSelector.css
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor: remove old pages, consolidate to unified Dashboard

Delete VideoProcessor, AdminDashboard, DvmDetail, PairDvm pages
and DvmSelector component. All functionality now in Dashboard
with DvmList, DvmDetailPanel, and PairDvmModal components.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Move SelfTest Styles to Component

**Files:**
- Create: `frontend/src/components/SelfTest.css`
- Modify: `frontend/src/components/SelfTest.tsx`

The SelfTest component's CSS was in VideoProcessor.css. Move it to its own file.

**Step 1: Create SelfTest.css with extracted styles**

```css
/* frontend/src/components/SelfTest.css */
.selftest-container {
  background: #1a1a1a;
  border: 1px solid #333;
  border-radius: 8px;
  padding: 1.25rem;
}

.selftest-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.selftest-header h3 {
  margin: 0;
  font-size: 1rem;
  color: #888;
}

.selftest-button {
  padding: 0.5rem 1rem;
  font-size: 0.875rem;
  font-weight: 500;
  background: #333;
  color: white;
  border: 1px solid #444;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.selftest-button:hover:not(:disabled) {
  background: #444;
  border-color: #555;
}

.selftest-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.selftest-running {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem 0;
  color: #888;
}

.selftest-running p {
  margin: 0;
  font-size: 0.875rem;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid #333;
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.selftest-result {
  border-radius: 6px;
  overflow: hidden;
}

.selftest-result.success {
  border: 1px solid rgba(52, 211, 153, 0.3);
}

.selftest-result.failure {
  border: 1px solid rgba(248, 113, 113, 0.3);
}

.result-header {
  padding: 0.75rem;
  background: rgba(0, 0, 0, 0.2);
}

.result-badge {
  font-size: 0.875rem;
  font-weight: 600;
}

.result-badge.success {
  color: #34d399;
}

.result-badge.failure {
  color: #f87171;
}

.result-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 0.75rem;
  padding: 1rem;
}

.result-item {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.result-label {
  font-size: 0.75rem;
  color: #666;
  text-transform: uppercase;
}

.result-value {
  font-size: 0.9rem;
  color: #ccc;
}

.result-value.highlight {
  color: #34d399;
  font-weight: 600;
  font-size: 1rem;
}

.error-message {
  color: #f87171;
  background: rgba(248, 113, 113, 0.1);
  border: 1px solid rgba(248, 113, 113, 0.3);
  padding: 0.75rem 1rem;
  border-radius: 6px;
  margin: 0;
}

/* System Info Section */
.system-info-section {
  margin-bottom: 1.5rem;
}

.system-info-section h3 {
  margin: 0 0 1rem 0;
  font-size: 1rem;
  color: #888;
}

.system-info-loading {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  color: #888;
  font-size: 0.875rem;
}

.system-info-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 0.75rem;
}

.system-info-card {
  background: #222;
  border: 1px solid #333;
  border-radius: 6px;
  padding: 0.75rem;
}

.system-info-card h4 {
  margin: 0 0 0.5rem 0;
  font-size: 0.75rem;
  color: #666;
  text-transform: uppercase;
  font-weight: 500;
}

.system-info-card .info-content {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.system-info-card .info-value {
  font-size: 0.9rem;
  color: #eee;
  font-weight: 500;
}

.system-info-card .info-value.error {
  color: #f87171;
}

.system-info-card .info-value.ffmpeg-version {
  font-family: monospace;
  font-size: 0.8rem;
}

.system-info-card .info-detail {
  font-size: 0.75rem;
  color: #888;
}

.system-info-card .info-detail.path {
  font-family: monospace;
  word-break: break-all;
  color: #666;
}

/* Disk space warnings */
.system-info-card.warning {
  border-color: #f59e0b;
  background: rgba(245, 158, 11, 0.1);
}

.system-info-card.critical {
  border-color: #f87171;
  background: rgba(248, 113, 113, 0.1);
}

/* Hardware encoder list */
.encoder-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.encoder-item {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  padding: 0.375rem 0.5rem;
  background: #1a1a1a;
  border-radius: 4px;
  border: 1px solid transparent;
}

.encoder-item.selected {
  border-color: #34d399;
  background: rgba(52, 211, 153, 0.1);
}

.encoder-name {
  font-size: 0.8rem;
  color: #ccc;
  display: flex;
  align-items: center;
  gap: 0.375rem;
}

.selected-badge {
  color: #34d399;
  font-size: 0.6rem;
}

.encoder-codecs {
  font-size: 0.7rem;
  color: #666;
}

/* Selftest section spacing */
.selftest-section {
  margin-top: 0.5rem;
  padding-top: 1rem;
  border-top: 1px solid #333;
}
```

**Step 2: Add import to SelfTest.tsx**

Add at line 1 of `frontend/src/components/SelfTest.tsx`:
```tsx
import "./SelfTest.css";
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add frontend/src/components/SelfTest.css frontend/src/components/SelfTest.tsx
git commit -m "$(cat <<'EOF'
refactor: extract SelfTest styles to own CSS file

Move SelfTest component styles from deleted VideoProcessor.css
to dedicated SelfTest.css file.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Final Testing and Cleanup

**Step 1: Run the development server and test manually**

Run: `cd frontend && npm run dev`

Test:
1. Visit http://localhost:5173
2. Should see login screen
3. Login with Nostr extension or nsec
4. Should see DVM list with filter toggle
5. "All DVMs" should show discovered DVMs
6. "My DVMs" should show owned DVMs with status
7. Selecting a public DVM shows basic info
8. Selecting owned DVM shows tabs: Overview, Config, Test Transcode, System
9. System tab shows SelfTest
10. Test Transcode tab allows submitting a video
11. Pair button in My DVMs opens modal

**Step 2: Run production build**

Run: `cd frontend && npm run build`
Expected: Build succeeds with no errors

**Step 3: Run linter**

Run: `cd frontend && npm run lint`
Expected: No errors (warnings acceptable)

**Step 4: Final commit if any fixes needed**

If there are any fixes:
```bash
git add -A
git commit -m "$(cat <<'EOF'
fix: address linting issues and polish UI

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Summary

After completing all tasks:

1. Single page at `/` with login required
2. DVM list with "All DVMs" / "My DVMs" filter toggle
3. Public DVMs show basic info only
4. Owned DVMs show admin features:
   - Overview with stats and recent jobs
   - Configuration editing
   - Test Transcode functionality
   - System info and Self-Test
5. Pairing via modal dialog
6. All legacy routes redirect to `/`
7. Removed ~6 files, added ~6 new component files
