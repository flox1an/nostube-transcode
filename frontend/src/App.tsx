import { useState, useCallback, useRef } from "react";
import type { Event } from "nostr-tools";
import { LoginButton } from "./components/LoginButton";
import { DvmSelector } from "./components/DvmSelector";
import { VideoForm, type OutputMode, type Resolution, type Codec } from "./components/VideoForm";
import { SelfTest } from "./components/SelfTest";
import { JobProgress, type StatusMessage } from "./components/JobProgress";
import { VideoPlayer } from "./components/VideoPlayer";
import { EventDisplay } from "./components/EventDisplay";
import { publishTransformRequest, subscribeToResponses } from "./nostr/client";
import { parseStatusEvent, parseResultEvent, type DvmResult } from "./nostr/events";
import type { DvmService } from "./nostr/discovery";
import "./App.css";

type AppState = "idle" | "submitting" | "processing" | "complete" | "error";

function truncatePubkey(pubkey: string): string {
  return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function App() {
  const [pubkey, setPubkey] = useState<string | null>(null);
  const [selectedDvm, setSelectedDvm] = useState<DvmService | null>(null);
  const [appState, setAppState] = useState<AppState>("idle");
  const [statusMessages, setStatusMessages] = useState<StatusMessage[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [dvmResult, setDvmResult] = useState<DvmResult | null>(null);
  const [requestEvent, setRequestEvent] = useState<Event | null>(null);
  const [responseEvent, setResponseEvent] = useState<Event | null>(null);

  const unsubscribeRef = useRef<(() => void) | null>(null);

  const handleDvmSelect = useCallback((dvm: DvmService) => {
    setSelectedDvm(dvm);
  }, []);

  const handleLogin = useCallback((pk: string) => {
    setPubkey(pk);
    setErrorMessage(null);
  }, []);

  const handleLoginError = useCallback((error: string) => {
    setErrorMessage(error);
  }, []);

  const handleLogout = useCallback(() => {
    // Cleanup subscription if any
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
      unsubscribeRef.current = null;
    }
    setPubkey(null);
    setAppState("idle");
    setStatusMessages([]);
    setErrorMessage(null);
    setDvmResult(null);
    setRequestEvent(null);
    setResponseEvent(null);
  }, []);

  const handleSubmit = useCallback(async (videoUrl: string, mode: OutputMode, resolution: Resolution, codec: Codec) => {
    if (!selectedDvm) {
      setErrorMessage("Please select a DVM first");
      return;
    }

    // Reset state
    setAppState("submitting");
    setStatusMessages([]);
    setErrorMessage(null);
    setDvmResult(null);
    setRequestEvent(null);
    setResponseEvent(null);

    // Cleanup previous subscription
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
      unsubscribeRef.current = null;
    }

    try {
      const { eventId, signedEvent } = await publishTransformRequest(
        videoUrl,
        selectedDvm.pubkey,
        selectedDvm.relays,
        mode,
        resolution,
        codec
      );
      setRequestEvent(signedEvent);
      setAppState("processing");

      // Subscribe to responses
      unsubscribeRef.current = subscribeToResponses(
        eventId,
        selectedDvm.pubkey,
        selectedDvm.relays,
        (response) => {
          if (response.type === "status") {
            const { status, message, eta } = parseStatusEvent(response.event);
            setStatusMessages((prev) => [
              ...prev,
              { status, message, timestamp: Date.now(), eta },
            ]);

            if (status === "error") {
              setAppState("error");
              setErrorMessage(message || "Job failed");
            }
          } else if (response.type === "result") {
            setResponseEvent(response.event);
            const result = parseResultEvent(response.event);
            if (result) {
              setDvmResult(result);
              setAppState("complete");
            }
          }
        }
      );

      // Set a timeout warning (not an error, just informational)
      setTimeout(() => {
        setStatusMessages((prev) => {
          // Only add timeout message if still processing and no messages yet
          if (prev.length === 0) {
            return [
              ...prev,
              {
                status: "waiting",
                message: "Waiting for DVM response...",
                timestamp: Date.now(),
              },
            ];
          }
          return prev;
        });
      }, 5000);
    } catch (err) {
      setAppState("error");
      setErrorMessage(err instanceof Error ? err.message : "Failed to submit request");
    }
  }, [selectedDvm]);

  const handleReset = useCallback(() => {
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
      unsubscribeRef.current = null;
    }
    setAppState("idle");
    setStatusMessages([]);
    setErrorMessage(null);
    setDvmResult(null);
    setRequestEvent(null);
    setResponseEvent(null);
  }, []);

  // Not logged in - show login screen
  if (!pubkey) {
    return (
      <div className="app">
        <div className="login-screen">
          <h1>DVM Video Processor</h1>
          <p>Transform videos to HLS format using Nostr</p>
          <LoginButton onLogin={handleLogin} onError={handleLoginError} />
          {errorMessage && <p className="error-message">{errorMessage}</p>}
          <SelfTest />
        </div>
      </div>
    );
  }

  // Logged in
  return (
    <div className="app">
      <header className="app-header">
        <h1>DVM Video Processor</h1>
        <div className="user-info">
          <span className="pubkey">{truncatePubkey(pubkey)}</span>
          <button className="logout-button" onClick={handleLogout}>
            Logout
          </button>
        </div>
      </header>

      <main className="app-main">
        <DvmSelector
          onSelect={handleDvmSelect}
          selectedDvm={selectedDvm}
          disabled={appState === "submitting" || appState === "processing"}
        />

        <VideoForm
          onSubmit={handleSubmit}
          disabled={appState === "submitting" || appState === "processing" || !selectedDvm}
        />

        {requestEvent && <EventDisplay event={requestEvent} />}

        <JobProgress messages={statusMessages} error={errorMessage || undefined} />

        {dvmResult && dvmResult.type === "hls" && (
          <VideoPlayer src={dvmResult.master_playlist} />
        )}

        {dvmResult && dvmResult.type === "mp4" && dvmResult.urls[0] && (
          <video
            className="mp4-player"
            src={dvmResult.urls[0]}
            controls
            playsInline
          />
        )}

        {responseEvent && <EventDisplay event={responseEvent} title="DVM Response Event" />}

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
                          <a href={stream.url} target="_blank" rel="noopener noreferrer">
                            View
                          </a>
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

        {(appState === "complete" || appState === "error") && (
          <button className="reset-button" onClick={handleReset}>
            Transform Another Video
          </button>
        )}
      </main>
    </div>
  );
}

export default App;
