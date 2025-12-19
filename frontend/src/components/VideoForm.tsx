import { useState } from "react";

export type OutputMode = "mp4" | "hls";
export type Resolution = "360p" | "480p" | "720p" | "1080p";

interface VideoFormProps {
  onSubmit: (url: string, mode: OutputMode, resolution: Resolution) => void;
  disabled: boolean;
}

export function VideoForm({ onSubmit, disabled }: VideoFormProps) {
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<OutputMode>("mp4");
  const [resolution, setResolution] = useState<Resolution>("720p");

  const isValidUrl = url.startsWith("https://");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (isValidUrl && !disabled) {
      onSubmit(url, mode, resolution);
    }
  };

  return (
    <form className="video-form" onSubmit={handleSubmit}>
      <input
        type="url"
        placeholder="https://blossom.example.com/video.mp4"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        disabled={disabled}
        className="url-input"
      />

      <div className="form-options">
        <div className="mode-toggle">
          <label className="option-label">Output:</label>
          <div className="toggle-buttons">
            <button
              type="button"
              className={`toggle-btn ${mode === "mp4" ? "active" : ""}`}
              onClick={() => setMode("mp4")}
              disabled={disabled}
            >
              MP4
            </button>
            <button
              type="button"
              className={`toggle-btn ${mode === "hls" ? "active" : ""}`}
              onClick={() => setMode("hls")}
              disabled={disabled}
            >
              HLS
            </button>
          </div>
        </div>

        {mode === "mp4" && (
          <div className="resolution-select">
            <label className="option-label">Resolution:</label>
            <select
              value={resolution}
              onChange={(e) => setResolution(e.target.value as Resolution)}
              disabled={disabled}
              className="resolution-dropdown"
            >
              <option value="360p">360p</option>
              <option value="480p">480p</option>
              <option value="720p">720p</option>
              <option value="1080p">1080p</option>
            </select>
          </div>
        )}
      </div>

      <button
        type="submit"
        disabled={disabled || !isValidUrl}
        className="submit-button"
      >
        {disabled ? "Processing..." : "Transform Video"}
      </button>
    </form>
  );
}
