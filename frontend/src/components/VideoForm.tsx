import { useState } from "react";

export type OutputMode = "mp4" | "hls";
export type Resolution = "360p" | "480p" | "720p" | "1080p";
export type Codec = "h264" | "h265";
export type HlsResolution = "240p" | "360p" | "480p" | "720p" | "1080p" | "original";

const ALL_HLS_RESOLUTIONS: HlsResolution[] = ["240p", "360p", "480p", "720p", "1080p", "original"];

interface VideoFormProps {
  onSubmit: (url: string, mode: OutputMode, resolution: Resolution, codec: Codec, hlsResolutions?: HlsResolution[], encryption?: boolean) => void;
  disabled: boolean;
}

export function VideoForm({ onSubmit, disabled }: VideoFormProps) {
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<OutputMode>("mp4");
  const [resolution, setResolution] = useState<Resolution>("720p");
  const [codec, setCodec] = useState<Codec>("h264");
  const [hlsResolutions, setHlsResolutions] = useState<Set<HlsResolution>>(
    new Set(ALL_HLS_RESOLUTIONS)
  );
  const [encryption, setEncryption] = useState(true);

  const isValidUrl = url.startsWith("https://");
  const isValidHlsSelection = hlsResolutions.size >= 2;

  const handleHlsResolutionToggle = (res: HlsResolution) => {
    setHlsResolutions((prev) => {
      const newSet = new Set(prev);
      if (newSet.has(res)) {
        newSet.delete(res);
      } else {
        newSet.add(res);
      }
      return newSet;
    });
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (isValidUrl && !disabled) {
      if (mode === "hls") {
        onSubmit(url, mode, resolution, codec, Array.from(hlsResolutions), encryption);
      } else {
        onSubmit(url, mode, resolution, codec);
      }
    }
  };

  const getResolutionLabel = (res: HlsResolution): string => {
    if (res === "original") return "Original";
    return res.toUpperCase();
  };

  return (
    <form className="video-form" onSubmit={handleSubmit}>
      <div className="form-section">
        <label className="section-label">Video URL</label>
        <input
          type="url"
          placeholder="https://blossom.example.com/video.mp4"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={disabled}
          className="url-input"
        />
      </div>

      <div className="form-section">
        <label className="section-label">Output Settings</label>
        <div className="form-row">
          <div className="form-field">
            <label className="field-label">Format</label>
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

          <div className="form-field">
            <label className="field-label">Codec</label>
            <div className="toggle-buttons">
              <button
                type="button"
                className={`toggle-btn ${codec === "h264" ? "active" : ""}`}
                onClick={() => setCodec("h264")}
                disabled={disabled}
              >
                H.264
              </button>
              <button
                type="button"
                className={`toggle-btn ${codec === "h265" ? "active" : ""}`}
                onClick={() => setCodec("h265")}
                disabled={disabled}
              >
                H.265
              </button>
            </div>
          </div>

          {mode === "mp4" && (
            <div className="form-field">
              <label className="field-label">Resolution</label>
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

          {mode === "hls" && (
            <div className="form-field">
              <label className="field-label">Encryption</label>
              <div className="toggle-buttons">
                <button
                  type="button"
                  className={`toggle-btn ${encryption ? "active" : ""}`}
                  onClick={() => setEncryption(true)}
                  disabled={disabled}
                >
                  On
                </button>
                <button
                  type="button"
                  className={`toggle-btn ${!encryption ? "active" : ""}`}
                  onClick={() => setEncryption(false)}
                  disabled={disabled}
                >
                  Off
                </button>
              </div>
              <span className="field-hint">
                {encryption ? "AES-128" : "fMP4"}
              </span>
            </div>
          )}
        </div>
      </div>

      {mode === "hls" && (
        <div className="form-section">
          <label className="section-label">HLS Resolutions</label>
          <div className="resolution-checkboxes">
            {ALL_HLS_RESOLUTIONS.map((res) => (
              <label
                key={res}
                className={`resolution-checkbox ${hlsResolutions.has(res) ? "checked" : ""}`}
              >
                <input
                  type="checkbox"
                  checked={hlsResolutions.has(res)}
                  onChange={() => handleHlsResolutionToggle(res)}
                  disabled={disabled}
                />
                <span className="checkbox-label">
                  {getResolutionLabel(res)}
                  {res === "original" && (
                    <span className="checkbox-hint">(passthrough)</span>
                  )}
                </span>
              </label>
            ))}
          </div>
          {!isValidHlsSelection && (
            <p className="validation-error">Select at least 2 resolutions for adaptive streaming</p>
          )}
        </div>
      )}

      <button
        type="submit"
        disabled={disabled || !isValidUrl || (mode === "hls" && !isValidHlsSelection)}
        className="submit-button"
      >
        {disabled ? "Processing..." : "Transform Video"}
      </button>
    </form>
  );
}
