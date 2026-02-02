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
            <SelfTest dvmPubkey={dvm.pubkey} userPubkey={userPubkey} />
          </div>
        )}
      </div>
    </div>
  );
}
