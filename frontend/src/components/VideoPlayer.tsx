import { useEffect, useRef, useState } from "react";
import Hls from "hls.js";

interface VideoPlayerProps {
  src: string;
}

interface QualityLevel {
  height: number;
  index: number;
}

export function VideoPlayer({ src }: VideoPlayerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [levels, setLevels] = useState<QualityLevel[]>([]);
  const [currentLevel, setCurrentLevel] = useState<number>(-1);
  const [selectedLevel, setSelectedLevel] = useState<number>(-1);

  const handleQualityChange = (level: number) => {
    setSelectedLevel(level);
    if (hlsRef.current) {
      hlsRef.current.currentLevel = level;
    }
  };

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    // Reset state on new src
    setError(null);
    setLevels([]);
    setCurrentLevel(-1);
    setSelectedLevel(-1);

    // Prefer hls.js when supported (gives us quality level control)
    // Only fall back to native HLS for browsers where hls.js doesn't work (e.g., iOS Safari)
    if (Hls.isSupported()) {
      const hls = new Hls();
      hlsRef.current = hls;

      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          setError("Failed to load video: " + data.details);
        }
      });

      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        const availableLevels = hls.levels
          .map((level, index) => ({
            height: level.height,
            index,
          }))
          .sort((a, b) => b.height - a.height);
        setLevels(availableLevels);
      });

      hls.on(Hls.Events.LEVEL_SWITCHED, (_event, data) => {
        setCurrentLevel(data.level);
      });

      hls.loadSource(src);
      hls.attachMedia(video);

      return () => {
        hlsRef.current = null;
        hls.destroy();
      };
    } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
      // Fallback to native HLS for iOS Safari (no quality selector available)
      video.src = src;
    } else {
      setError("HLS playback is not supported in this browser");
    }
  }, [src]);

  if (error) {
    return (
      <div className="video-player video-error">
        <p>{error}</p>
        <p>
          <a href={src} target="_blank" rel="noopener noreferrer">
            Open video URL directly
          </a>
        </p>
      </div>
    );
  }

  const getLevelLabel = (level: QualityLevel) => `${level.height}p`;

  return (
    <div className="video-player">
      <video ref={videoRef} controls playsInline />
      {levels.length > 0 && (
        <div className="quality-selector">
          <select
            value={selectedLevel}
            onChange={(e) => handleQualityChange(Number(e.target.value))}
          >
            <option value={-1}>
              Auto{selectedLevel === -1 && currentLevel >= 0 ? ` (${levels.find(l => l.index === currentLevel)?.height}p)` : ""}
            </option>
            {levels.map((level) => (
              <option key={level.index} value={level.index}>
                {getLevelLabel(level)}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}
