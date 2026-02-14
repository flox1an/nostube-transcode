import "./SelfTest.css";
import { useState, useEffect, useCallback, useRef } from "react";
import {
  sendAdminCommand,
  subscribeToAdminResponses,
  type AdminResponseWire,
  type SelfTestResult,
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
  const [result, setResult] = useState<SelfTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfoResult | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(true);
  const [systemInfoError, setSystemInfoError] = useState<string | null>(null);

  const subscriptionRef = useRef<(() => void) | null>(null);
  const pendingCommandRef = useRef<"system_info" | "self_test" | null>(null);

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
    // Check if this is a self_test response
    else if ("success" in data && ("speed_ratio" in data || "error" in data)) {
      setResult(data as unknown as SelfTestResult);
      setIsRunning(false);
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
    pendingCommandRef.current = "self_test";

    try {
      await sendAdminCommand(signer, dvmPubkey, "self_test", {}, RELAYS);

      // Set a timeout for self-test (it can take a while)
      setTimeout(() => {
        if (isRunning && pendingCommandRef.current === "self_test") {
          setError("Timeout waiting for self-test response (test may still be running)");
          setIsRunning(false);
          pendingCommandRef.current = null;
        }
      }, 120000); // 2 minute timeout
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run self-test");
      setIsRunning(false);
      pendingCommandRef.current = null;
    }
  }, [dvmPubkey, isRunning]);

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
          <button
            className="selftest-button"
            onClick={runSelfTest}
            disabled={isRunning}
          >
            {isRunning ? "Running..." : "Run Test"}
          </button>
        </div>

        {isRunning && (
          <div className="selftest-running">
            <div className="spinner" />
            <p>Encoding test video at 720p... This may take a minute.</p>
          </div>
        )}

        {error && <p className="error-message">{error}</p>}

        {result && (
          <div className={`selftest-result ${result.success ? "success" : "failure"}`}>
            <div className="result-header">
              <span className={`result-badge ${result.success ? "success" : "failure"}`}>
                {result.success ? "✓ Passed" : "✗ Failed"}
              </span>
            </div>

            {result.error && (
              <p className="error-message">{result.error}</p>
            )}

            {result.success && (
              <div className="result-grid">
                <div className="result-item">
                  <span className="result-label">Speed</span>
                  <span className="result-value highlight">{result.speed_ratio?.toFixed(1) ?? "N/A"}x realtime</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Hardware Accel</span>
                  <span className="result-value">{result.hwaccel ?? "N/A"}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Video Duration</span>
                  <span className="result-value">{result.video_duration_secs ? formatDuration(result.video_duration_secs) : "N/A"}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Encode Time</span>
                  <span className="result-value">{result.encode_time_secs ? formatDuration(result.encode_time_secs) : "N/A"}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Resolution</span>
                  <span className="result-value">{result.resolution ?? "N/A"}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Output Size</span>
                  <span className="result-value">{result.output_size_bytes ? formatBytes(result.output_size_bytes) : "N/A"}</span>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
