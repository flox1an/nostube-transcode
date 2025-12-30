import { useState } from "react";

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

  return (
    <div className="selftest-container">
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
  );
}
