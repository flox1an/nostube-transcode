import { useState, useEffect } from "react";

interface SelfTestResult {
  success: boolean;
  test_video_url: string;
  video_duration_secs: number;
  encode_time_secs: number;
  speed_ratio: number;
  speed_description: string;
  hwaccel: string;
  resolution: string;
  output_size_bytes: number;
  error?: string;
}

interface HwEncoderInfo {
  name: string;
  selected: boolean;
  codecs: string[];
}

interface GpuInfo {
  name: string;
  vendor: string;
  details?: string;
}

interface DiskInfo {
  path: string;
  free_bytes: number;
  total_bytes: number;
  free_percent: number;
}

interface FfmpegInfo {
  path: string;
  version?: string;
  ffprobe_path: string;
}

interface SystemInfo {
  platform: string;
  arch: string;
  hw_encoders: HwEncoderInfo[];
  gpu?: GpuInfo;
  disk: DiskInfo;
  ffmpeg: FfmpegInfo;
  temp_dir: string;
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

export function SelfTest() {
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<SelfTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(true);
  const [systemInfoError, setSystemInfoError] = useState<string | null>(null);

  // Fetch system info on mount
  useEffect(() => {
    const fetchSystemInfo = async () => {
      try {
        const response = await fetch("/system-info");
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }
        const data: SystemInfo = await response.json();
        setSystemInfo(data);
      } catch (err) {
        setSystemInfoError(err instanceof Error ? err.message : "Failed to fetch system info");
      } finally {
        setSystemInfoLoading(false);
      }
    };
    fetchSystemInfo();
  }, []);

  const runSelfTest = async () => {
    setIsRunning(true);
    setResult(null);
    setError(null);

    try {
      const response = await fetch("/selftest");
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      const data: SelfTestResult = await response.json();
      setResult(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run self-test");
    } finally {
      setIsRunning(false);
    }
  };

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
                {systemInfo.hw_encoders.map((enc, i) => (
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
                  <span className="info-value">{systemInfo.gpu.name}</span>
                  <span className="info-detail">{systemInfo.gpu.vendor}</span>
                  {systemInfo.gpu.details && (
                    <span className="info-detail">{systemInfo.gpu.details}</span>
                  )}
                </div>
              </div>
            )}

            <div className={`system-info-card ${getDiskWarningClass(systemInfo.disk.free_percent)}`}>
              <h4>Disk Space</h4>
              <div className="info-content">
                <span className="info-value">
                  {formatBytes(systemInfo.disk.free_bytes)} free
                </span>
                <span className="info-detail">
                  of {formatBytes(systemInfo.disk.total_bytes)} ({systemInfo.disk.free_percent.toFixed(1)}%)
                </span>
                <span className="info-detail path">{systemInfo.disk.path}</span>
              </div>
            </div>

            <div className="system-info-card">
              <h4>FFmpeg</h4>
              <div className="info-content">
                {systemInfo.ffmpeg.version ? (
                  <span className="info-value ffmpeg-version">
                    {systemInfo.ffmpeg.version.split(" ").slice(0, 3).join(" ")}
                  </span>
                ) : (
                  <span className="info-value error">Not found</span>
                )}
                <span className="info-detail path">{systemInfo.ffmpeg.path}</span>
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
                  <span className="result-value highlight">{result.speed_ratio.toFixed(1)}x realtime</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Hardware Accel</span>
                  <span className="result-value">{result.hwaccel}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Video Duration</span>
                  <span className="result-value">{formatDuration(result.video_duration_secs)}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Encode Time</span>
                  <span className="result-value">{formatDuration(result.encode_time_secs)}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Resolution</span>
                  <span className="result-value">{result.resolution}</span>
                </div>
                <div className="result-item">
                  <span className="result-label">Output Size</span>
                  <span className="result-value">{formatBytes(result.output_size_bytes)}</span>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
