// frontend/src/components/DvmDetailPanel.tsx
import { useState, useEffect, useCallback, useRef } from "react";
import type { Event } from "nostr-tools";
import type { UnifiedDvm } from "./DvmList";
import {
  sendAdminCommand,
  subscribeToAdminResponses,
  type DvmConfig,
  type DvmStatus,
  type DvmJob,
  type DvmDashboard,
  type AdminResponseWire,
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
import { UserAvatar } from "./UserAvatar";
import { UserName } from "./UserName";
import { IconClock, IconRefresh, IconCheckCircle, IconXCircle } from "./Icons";
import "./DvmDetailPanel.css";

type TabType = "overview" | "config" | "transcode" | "system";
type PublicTabType = "overview" | "transcode";

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
  const [publicTab, setPublicTab] = useState<PublicTabType>("overview");
  const [status, setStatus] = useState<DvmStatus | null>(dvm.status || null);
  const [config, setConfig] = useState<DvmConfig | null>(null);
  const [jobs, setJobs] = useState<DvmJob[]>([]);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [offline, setOffline] = useState(false);

  // Config editing
  const [editingConfig, setEditingConfig] = useState(false);
  const [formName, setFormName] = useState("");
  const [formAbout, setFormAbout] = useState("");
  const [formRelays, setFormRelays] = useState("");
  const [formBlossom, setFormBlossom] = useState("");
  const [formExpiration, setFormExpiration] = useState("");

  const startEditing = useCallback(() => {
    if (!config) return;
    setFormName(config.name || "");
    setFormAbout(config.about || "");
    setFormRelays(config.relays.join("\n"));
    setFormBlossom(config.blossom_servers.join("\n"));
    setFormExpiration(config.blob_expiration_days.toString());
    setEditingConfig(true);
  }, [config]);

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

  const handleAdminResponse = useCallback((response: AdminResponseWire) => {
    if (response.error) {
      console.error("Admin command failed:", response.error);
      return;
    }

    const data = response.result as Record<string, unknown>;
    if (!data) return;

    setOffline(false);

    // Dashboard response (status + config + jobs)
    if ("status" in data && "config" in data && "jobs" in data) {
      const dashboard = data as unknown as DvmDashboard;
      setStatus(dashboard.status);
      setConfig(dashboard.config);
      setJobs(dashboard.jobs);
    }
    // Status response (from status, pause, or resume commands)
    else if ("paused" in data && "jobs_active" in data) {
      setStatus(data as unknown as DvmStatus);
    }
    // Config response (from set_config)
    else if ("config" in data) {
      const cfg = data.config as DvmConfig;
      setConfig(cfg);
    }
    // Job history response
    else if ("jobs" in data) {
      setJobs(data.jobs as DvmJob[]);
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

    sendAdminCommand(signer, dvm.pubkey, "get_dashboard", { limit: 20 }, RELAYS)
      .then(() => setTimeout(() => setLoading(false), 1500))
      .catch((err) => {
        console.error("Failed to fetch DVM data:", err);
        setLoading(false);
      });

    const timeout = setTimeout(() => {
      setLoading((prev) => {
        if (prev) {
          setOffline(true);
          return false;
        }
        return prev;
      });
    }, 10000);

    return () => {
      clearTimeout(timeout);
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
      const method = status.paused ? "resume" : "pause";
      await sendAdminCommand(signer, dvm.pubkey, method, {}, RELAYS);
      // Status will be updated via the response handler (pause/resume now return status)
      setTimeout(() => setActionLoading(false), 1000);
    } catch (err) {
      console.error("Failed to pause/resume:", err);
      setActionLoading(false);
    }
  }, [dvm.pubkey, status]);

  const handleSaveConfig = useCallback(async () => {
    const signer = getCurrentSigner();
    if (!signer) return;

    setActionLoading(true);
    try {
      const relays = formRelays.split("\n").map(r => r.trim()).filter(r => r);
      const blossom = formBlossom.split("\n").map(s => s.trim()).filter(s => s);
      const expiration = parseInt(formExpiration, 10);

      await sendAdminCommand(signer, dvm.pubkey, "set_config", {
        relays: relays.length > 0 ? relays : undefined,
        blossom_servers: blossom.length > 0 ? blossom : undefined,
        blob_expiration_days: isNaN(expiration) ? undefined : expiration,
        name: formName || undefined,
        about: formAbout || undefined,
      }, RELAYS);
      
      setEditingConfig(false);
      setTimeout(() => setActionLoading(false), 1000);
    } catch (err) {
      console.error("Failed to save config:", err);
      setActionLoading(false);
    }
  }, [dvm.pubkey, formName, formAbout, formRelays, formBlossom, formExpiration]);

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

  if (!dvm.isOwned) {
    return (
      <div className="dvm-detail-panel">
        <div className="dvm-detail-header">
          <div className="header-left">
            <div className="title-with-avatar">
              <UserAvatar pubkey={dvm.pubkey} size={48} />
              <div className="title-text">
                <h2>{dvm.name || <UserName pubkey={dvm.pubkey} />}</h2>
                <code className="npub"><UserName pubkey={dvm.pubkey} /></code>
              </div>
            </div>
          </div>
        </div>

        <div className="detail-tabs">
          <button className={publicTab === "overview" ? "active" : ""} onClick={() => setPublicTab("overview")}>
            Overview
          </button>
          <button className={publicTab === "transcode" ? "active" : ""} onClick={() => setPublicTab("transcode")}>
            Transcode
          </button>
        </div>

        <div className="detail-content">
          {publicTab === "overview" && (
            <div className="public-dvm-info">
              <p className="dvm-about">{dvm.about || "No description"}</p>
              {dvm.supportedModes && (
                <p><strong>Modes:</strong> {dvm.supportedModes.join(", ")}</p>
              )}
              {dvm.supportedResolutions && (
                <p><strong>Resolutions:</strong> {dvm.supportedResolutions.join(", ")}</p>
              )}
              {dvm.operatorPubkey && (
                <div className="operator-info">
                  <span>Operated by:</span>
                  <UserAvatar pubkey={dvm.operatorPubkey} size={24} />
                  <UserName pubkey={dvm.operatorPubkey} />
                </div>
              )}
            </div>
          )}

          {publicTab === "transcode" && (
            <div className="transcode-tab">
              <p className="tab-description">
                Submit a video to this DVM for transcoding.
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
                  Transcode Another Video
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  // Owned DVM view with tabs
  return (
    <div className="dvm-detail-panel">
      <div className="dvm-detail-header">
        <div className="header-left">
          <div className="title-with-avatar">
            <UserAvatar pubkey={dvm.pubkey} size={48} />
            <div className="title-text">
              <h2>{config?.name || dvm.name || <UserName pubkey={dvm.pubkey} />}</h2>
              <code className="npub"><UserName pubkey={dvm.pubkey} /></code>
            </div>
          </div>
        </div>
        <div className="header-actions">
          {offline ? (
            <span className="status-badge offline">Offline</span>
          ) : status ? (
            <span className={`status-badge ${status.paused ? "paused" : "active"}`}>
              {status.paused ? "Paused" : "Active"}
            </span>
          ) : null}
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
          Transcode
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
                    <div className="stat-icon uptime-icon"><IconClock /></div>
                    <div className="stat-content">
                      <h3>Uptime</h3>
                      <p className="stat-value">{formatUptime(status.uptime_secs)}</p>
                    </div>
                  </div>
                  <div className="stat-card">
                    <div className="stat-icon active-icon"><IconRefresh /></div>
                    <div className="stat-content">
                      <h3>Active Jobs</h3>
                      <p className="stat-value">{status.jobs_active}</p>
                    </div>
                  </div>
                  <div className="stat-card">
                    <div className="stat-icon complete-icon"><IconCheckCircle /></div>
                    <div className="stat-content">
                      <h3>Completed</h3>
                      <p className="stat-value">{status.jobs_completed}</p>
                    </div>
                  </div>
                  <div className="stat-card">
                    <div className="stat-icon failed-icon"><IconXCircle /></div>
                    <div className="stat-content">
                      <h3>Failed</h3>
                      <p className="stat-value">{status.jobs_failed}</p>
                    </div>
                  </div>
                </div>

                <div className="overview-info-grid">
                  <div className="info-section">
                    <h3>Performance</h3>
                    <div className="performance-stats">
                      <div className="perf-item">
                        <span className="label">Total Jobs:</span>
                        <span className="value">{status.jobs_completed + status.jobs_failed + status.jobs_active}</span>
                      </div>
                      <div className="perf-item">
                        <span className="label">Success Rate:</span>
                        <span className="value">
                          {status.jobs_completed + status.jobs_failed > 0 
                            ? `${Math.round((status.jobs_completed / (status.jobs_completed + status.jobs_failed)) * 100)}%`
                            : "N/A"}
                        </span>
                      </div>
                    </div>
                  </div>

                  <div className="info-section">
                    <h3>System</h3>
                    <div className="system-details">
                      <div className="sys-item">
                        <span className="label">HW Acceleration:</span>
                        <span className="value accent">{status.hwaccel || "None"}</span>
                      </div>
                      <div className="sys-item">
                        <span className="label">Version:</span>
                        <span className="value">{status.version}</span>
                      </div>
                    </div>
                  </div>
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

                <button className="edit-btn" onClick={startEditing}>
                  Edit Configuration
                </button>
              </>
            ) : (
              <div className="config-form">
                <div className="form-group">
                  <label>Name</label>
                  <input
                    type="text"
                    placeholder="DVM Name"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                  />
                </div>

                <div className="form-group">
                  <label>About</label>
                  <textarea
                    placeholder="Describe your DVM..."
                    value={formAbout}
                    onChange={(e) => setFormAbout(e.target.value)}
                  />
                </div>

                <div className="form-group">
                  <label>Relays (one per line)</label>
                  <textarea
                    placeholder="wss://relay.damus.io"
                    value={formRelays}
                    onChange={(e) => setFormRelays(e.target.value)}
                  />
                  <p className="form-help">Enter one Nostr relay URL per line. These are the relays the DVM listens on.</p>
                </div>

                <div className="form-group">
                  <label>Blossom Servers (one per line)</label>
                  <textarea
                    placeholder="https://blossom.example.com"
                    value={formBlossom}
                    onChange={(e) => setFormBlossom(e.target.value)}
                  />
                  <p className="form-help">Enter one Blossom server URL per line. These are where transformed videos are uploaded.</p>
                </div>

                <div className="form-group">
                  <label>Blob Expiration (days)</label>
                  <input
                    type="number"
                    min="1"
                    value={formExpiration}
                    onChange={(e) => setFormExpiration(e.target.value)}
                  />
                  <p className="form-help">Number of days before uploaded video segments expire on Blossom servers.</p>
                </div>

                <div className="form-actions">
                  <button className="save-btn" onClick={handleSaveConfig} disabled={actionLoading}>
                    {actionLoading ? "Saving..." : "Save Changes"}
                  </button>
                  <button
                    className="cancel-btn"
                    onClick={() => setEditingConfig(false)}
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
            <p className="tab-description">
              Submit a video to this DVM for transcoding.
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
                Transcode Another Video
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
