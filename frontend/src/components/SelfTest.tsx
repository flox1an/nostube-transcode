import "./SelfTest.css";
import { useState, useEffect, useCallback, useRef } from "react";
import {
  sendAdminCommand,
  subscribeToAdminResponses,
  type AdminResponseWire,
  type SelfTestSuiteResult,
  type SelfTestResultEntry,
  type SelfTestCheck,
  type SystemInfoResult,
  type HwEncoderInfo,
  type GpuInfo,
  type DiskInfo,
  type FfmpegInfo,
} from "../nostr/admin";
import { getCurrentSigner } from "../nostr/client";
import { RELAYS } from "../nostr/constants";

interface SelfTestProps {
  dvmPubkey: string;
  userPubkey: string;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const mins = Math.floor(secs / 60);
  const remainingSecs = secs % 60;
  return `${mins}m ${remainingSecs.toFixed(0)}s`;
}

export function SelfTest({ dvmPubkey, userPubkey }: SelfTestProps) {
  const [isRunning, setIsRunning] = useState(false);
  const [mode, setMode] = useState<"quick" | "full">("quick");
  const [result, setResult] = useState<SelfTestSuiteResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfoResult | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(true);
  const [systemInfoError, setSystemInfoError] = useState<string | null>(null);
  const [expandedRows, setExpandedRows] = useState<Set<number>>(new Set());

  const subscriptionRef = useRef<(() => void) | null>(null);
  const pendingCommandRef = useRef<"system_info" | "self_test" | null>(null);

  const toggleRow = (index: number) => {
    setExpandedRows((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  const handleResponse = useCallback((response: AdminResponseWire) => {
    if (response.error) {
      console.error("Admin command failed:", response.error);
      return;
    }

    const data = response.result as Record<string, unknown>;
    if (!data) return;

    // Check if this is a system_info response
    if ("platform" in data && "arch" in data) {
      setSystemInfo(data as unknown as SystemInfoResult);
      setSystemInfoLoading(false);
      if (pendingCommandRef.current === "system_info") {
        pendingCommandRef.current = null;
      }
    }
    // Check if this is a self_test suite response
    else if ("summary" in data && "results" in data) {
      setResult(data as unknown as SelfTestSuiteResult);
      setIsRunning(false);
      setExpandedRows(new Set());
      if (pendingCommandRef.current === "self_test") {
        pendingCommandRef.current = null;
      }
    }
  }, []);

  // Set up subscription and fetch system info on mount
  useEffect(() => {
    const signer = getCurrentSigner();
    if (!signer) {
      setSystemInfoError("No signer available");
      setSystemInfoLoading(false);
      return;
    }

    // Subscribe to responses from DVM
    const unsubscribe = subscribeToAdminResponses(
      signer,
      userPubkey,
      dvmPubkey,
      RELAYS,
      handleResponse
    );
    subscriptionRef.current = unsubscribe;

    // Fetch system info
    pendingCommandRef.current = "system_info";
    sendAdminCommand(signer, dvmPubkey, "system_info", {}, RELAYS)
      .catch((err) => {
        setSystemInfoError(err instanceof Error ? err.message : "Failed to fetch system info");
        setSystemInfoLoading(false);
        pendingCommandRef.current = null;
      });

    // Timeout for system info
    const timeout = setTimeout(() => {
      if (systemInfoLoading && pendingCommandRef.current === "system_info") {
        setSystemInfoError("Timeout waiting for system info response");
        setSystemInfoLoading(false);
        pendingCommandRef.current = null;
      }
    }, 15000);

    return () => {
      clearTimeout(timeout);
      if (subscriptionRef.current) {
        subscriptionRef.current();
        subscriptionRef.current = null;
      }
    };
  }, [dvmPubkey, userPubkey, handleResponse, systemInfoLoading]);

  const runSelfTest = useCallback(async () => {
    const signer = getCurrentSigner();
    if (!signer) {
      setError("No signer available");
      return;
    }

    setIsRunning(true);
    setResult(null);
    setError(null);
    setExpandedRows(new Set());
    pendingCommandRef.current = "self_test";

    const timeoutMs = mode === "full" ? 300000 : 120000; // 5 min for full, 2 min for quick

    try {
      await sendAdminCommand(signer, dvmPubkey, "self_test", { mode }, RELAYS);

      setTimeout(() => {
        if (isRunning && pendingCommandRef.current === "self_test") {
          setError("Timeout waiting for self-test response (test may still be running)");
          setIsRunning(false);
          pendingCommandRef.current = null;
        }
      }, timeoutMs);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run self-test");
      setIsRunning(false);
      pendingCommandRef.current = null;
    }
  }, [dvmPubkey, isRunning, mode]);

  const getDiskWarningClass = (freePercent: number) => {
    if (freePercent < 10) return "critical";
    if (freePercent < 20) return "warning";
    return "";
  };

  return (
    <div className="selftest-container">
      {/* System Info Section */}
      <div className="system-info-section">
        <h3>System Information</h3>
        {systemInfoLoading && (
          <div className="system-info-loading">
            <div className="spinner" />
            <span>Loading system info...</span>
          </div>
        )}
        {systemInfoError && <p className="error-message">{systemInfoError}</p>}
        {systemInfo && (
          <div className="system-info-grid">
            <div className="system-info-card">
              <h4>Platform</h4>
              <div className="info-content">
                <span className="info-value">{systemInfo.platform}</span>
                <span className="info-detail">{systemInfo.arch}</span>
              </div>
            </div>

            <div className="system-info-card">
              <h4>Hardware Encoders</h4>
              <div className="encoder-list">
                {systemInfo.hw_encoders.map((enc: HwEncoderInfo, i: number) => (
                  <div key={i} className={`encoder-item ${enc.selected ? "selected" : ""}`}>
                    <span className="encoder-name">
                      {enc.selected && <span className="selected-badge">●</span>}
                      {enc.name}
                    </span>
                    <span className="encoder-codecs">{enc.codecs.join(", ")}</span>
                  </div>
                ))}
              </div>
            </div>

            <div className="system-info-card">
              <h4>AV1 Hardware Decode</h4>
              <div className="info-content">
                <span className={`info-value ${systemInfo.av1_hw_decode ? "success" : ""}`}>
                  {systemInfo.av1_hw_decode ? "Available" : "Not available"}
                </span>
              </div>
            </div>

            {systemInfo.gpu && (
              <div className="system-info-card">
                <h4>GPU</h4>
                <div className="info-content">
                  <span className="info-value">{(systemInfo.gpu as GpuInfo).name}</span>
                  <span className="info-detail">{(systemInfo.gpu as GpuInfo).vendor}</span>
                  {(systemInfo.gpu as GpuInfo).details && (
                    <span className="info-detail">{(systemInfo.gpu as GpuInfo).details}</span>
                  )}
                </div>
              </div>
            )}

            <div className={`system-info-card ${getDiskWarningClass((systemInfo.disk as DiskInfo).free_percent)}`}>
              <h4>Disk Space</h4>
              <div className="info-content">
                <span className="info-value">
                  {formatBytes((systemInfo.disk as DiskInfo).free_bytes)} free
                </span>
                <span className="info-detail">
                  of {formatBytes((systemInfo.disk as DiskInfo).total_bytes)} ({(systemInfo.disk as DiskInfo).free_percent.toFixed(1)}%)
                </span>
                <span className="info-detail path">{(systemInfo.disk as DiskInfo).path}</span>
              </div>
            </div>

            <div className="system-info-card">
              <h4>FFmpeg</h4>
              <div className="info-content">
                {(systemInfo.ffmpeg as FfmpegInfo).version ? (
                  <span className="info-value ffmpeg-version">
                    {(systemInfo.ffmpeg as FfmpegInfo).version!.split(" ").slice(0, 3).join(" ")}
                  </span>
                ) : (
                  <span className="info-value error">Not found</span>
                )}
                <span className="info-detail path">{(systemInfo.ffmpeg as FfmpegInfo).path}</span>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Encoder Self-Test Section */}
      <div className="selftest-section">
        <div className="selftest-header">
          <h3>Encoder Self-Test</h3>
          <div className="selftest-controls">
            <div className="mode-toggle">
              <button
                className={`mode-button ${mode === "quick" ? "active" : ""}`}
                onClick={() => setMode("quick")}
                disabled={isRunning}
              >
                Quick
              </button>
              <button
                className={`mode-button ${mode === "full" ? "active" : ""}`}
                onClick={() => setMode("full")}
                disabled={isRunning}
              >
                Full
              </button>
            </div>
            <button
              className="selftest-button"
              onClick={runSelfTest}
              disabled={isRunning}
            >
              {isRunning ? "Running..." : "Run Test"}
            </button>
          </div>
        </div>

        {isRunning && (
          <div className="selftest-running">
            <div className="spinner" />
            <p>Running {mode} self-test... This may take {mode === "full" ? "several minutes" : "a minute"}.</p>
          </div>
        )}

        {error && <p className="error-message">{error}</p>}

        {result && (
          <div className="selftest-results">
            {/* Summary bar */}
            <div className={`summary-bar ${result.summary.failed > 0 ? "has-failures" : ""}`}>
              <span className="summary-counts">
                <span className={`result-badge ${result.summary.passed === result.summary.total ? "success" : "failure"}`}>
                  {result.summary.passed}/{result.summary.total} passed
                </span>
                {result.summary.skipped > 0 && (
                  <span className="summary-skipped">{result.summary.skipped} skipped</span>
                )}
              </span>
              <span className="summary-meta">
                Total: {formatDuration(result.summary.duration_secs)} | HW: {result.hwaccel} | Mode: {result.mode}
              </span>
            </div>

            {/* Results table */}
            <table className="results-table">
              <thead>
                <tr>
                  <th>Clip</th>
                  <th>Output Codec</th>
                  <th>Status</th>
                  <th>Speed</th>
                  <th>Time</th>
                </tr>
              </thead>
              <tbody>
                {result.results.map((entry: SelfTestResultEntry, i: number) => (
                  <>
                    <tr
                      key={`row-${i}`}
                      className={`result-row ${entry.passed ? "passed" : "failed"} ${expandedRows.has(i) ? "expanded" : ""}`}
                      onClick={() => toggleRow(i)}
                    >
                      <td>{entry.clip_name}</td>
                      <td><code>{entry.output_codec}</code></td>
                      <td>
                        <span className={`result-badge ${entry.passed ? "success" : "failure"}`}>
                          {entry.passed ? "PASS" : "FAIL"}
                        </span>
                      </td>
                      <td>{entry.speed_ratio.toFixed(1)}x</td>
                      <td>{formatDuration(entry.encode_time_secs)}</td>
                    </tr>
                    {expandedRows.has(i) && (
                      <tr key={`detail-${i}`} className="detail-row">
                        <td colSpan={5}>
                          {entry.error && (
                            <p className="error-message">{entry.error}</p>
                          )}
                          {entry.checks.length > 0 && (
                            <div className="checks-list">
                              {entry.checks.map((check: SelfTestCheck, j: number) => (
                                <div key={j} className={`check-row ${check.passed ? "passed" : "failed"}`}>
                                  <span className={`check-icon ${check.passed ? "success" : "failure"}`}>
                                    {check.passed ? "OK" : "FAIL"}
                                  </span>
                                  <span className="check-name">{check.name}</span>
                                  <span className="check-detail">{check.detail}</span>
                                </div>
                              ))}
                            </div>
                          )}
                        </td>
                      </tr>
                    )}
                  </>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
