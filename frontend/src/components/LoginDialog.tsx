import { useState, useRef, useEffect, useCallback } from "react";
import { nip19 } from "nostr-tools";
import { QRCodeSVG } from "qrcode.react";
import { NostrConnectSigner } from "applesauce-signers/signers";
import { useCurrentUser } from "../hooks/useCurrentUser";
import { IconQrCode, IconCpu, IconDatabase, IconKey, IconAlertTriangle, IconFileText } from "./Icons";
import { RELAYS } from "../nostr/constants";

type LoginMethod = "nostrconnect" | "extension" | "bunker" | "nsec";

interface LoginDialogProps {
  onLogin: () => void;
  onError: (error: string) => void;
}

export function LoginDialog({ onLogin, onError }: LoginDialogProps) {
  const { loginWithExtension, loginWithNsec, loginWithBunker, loginWithNostrConnect } = useCurrentUser();
  const [activeTab, setActiveTab] = useState<LoginMethod>("nostrconnect");
  const [isLoading, setIsLoading] = useState(false);
  const [nsec, setNsec] = useState("");
  const [bunkerUri, setBunkerUri] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Nostr Connect state
  const [connectUri, setConnectUri] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const cleanupConnect = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setConnectUri(null);
  }, []);

  const initNostrConnect = useCallback(async () => {
    cleanupConnect();

    const signer = new NostrConnectSigner({
      relays: RELAYS,
    });

    const uri = signer.getNostrConnectURI({
      name: "Nostube Transform",
      permissions: ["sign_event", "nip44_encrypt", "nip44_decrypt"],
    });

    setConnectUri(uri);

    const abort = new AbortController();
    abortRef.current = abort;

    try {
      await signer.waitForSigner(abort.signal);
      if (!abort.signal.aborted) {
        await loginWithNostrConnect(signer);
        onLogin();
      }
    } catch (err) {
      if (!abort.signal.aborted) {
        onError(err instanceof Error ? err.message : "Nostr Connect failed");
        cleanupConnect();
      }
    }
  }, [cleanupConnect, loginWithNostrConnect, onLogin, onError]);

  // Start Nostr Connect when tab is active
  useEffect(() => {
    if (activeTab === "nostrconnect") {
      initNostrConnect();
    } else {
      cleanupConnect();
    }
    return () => cleanupConnect();
  }, [activeTab]); // eslint-disable-line react-hooks/exhaustive-deps

  const hasExtension = () => {
    return typeof window !== "undefined" && "nostr" in window;
  };

  const handleExtensionLogin = async () => {
    if (!hasExtension()) {
      onError("No Nostr extension found. Install Alby or nos2x.");
      return;
    }

    setIsLoading(true);
    try {
      await loginWithExtension();
      onLogin();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Extension login failed");
    } finally {
      setIsLoading(false);
    }
  };

  const handleNsecLogin = async () => {
    if (!nsec.trim()) {
      onError("Please enter your nsec key");
      return;
    }

    try {
      const decoded = nip19.decode(nsec.trim());
      if (decoded.type !== "nsec") {
        onError("Invalid nsec format. Key must start with 'nsec1'");
        return;
      }
    } catch {
      onError("Invalid nsec format. Please check your key.");
      return;
    }

    setIsLoading(true);
    try {
      await loginWithNsec(nsec.trim());
      setNsec("");
      onLogin();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Nsec login failed");
    } finally {
      setIsLoading(false);
    }
  };

  const handleBunkerLogin = async () => {
    if (!bunkerUri.trim()) {
      onError("Please enter a bunker URI");
      return;
    }

    if (!bunkerUri.startsWith("bunker://")) {
      onError("Bunker URI must start with bunker://");
      return;
    }

    setIsLoading(true);
    try {
      await loginWithBunker(bunkerUri.trim());
      setBunkerUri("");
      onLogin();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Bunker login failed");
    } finally {
      setIsLoading(false);
    }
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (event) => {
      const content = event.target?.result as string;
      setNsec(content.trim());
    };
    reader.readAsText(file);
  };

  return (
    <div className="login-dialog">
      <div className="login-tabs">
        <button
          className={`login-tab ${activeTab === "nostrconnect" ? "active" : ""}`}
          onClick={() => setActiveTab("nostrconnect")}
        >
          <IconQrCode className="tab-icon" /> Connect
        </button>
        <button
          className={`login-tab ${activeTab === "extension" ? "active" : ""}`}
          onClick={() => setActiveTab("extension")}
        >
          <IconCpu className="tab-icon" /> Extension
        </button>
        <button
          className={`login-tab ${activeTab === "bunker" ? "active" : ""}`}
          onClick={() => setActiveTab("bunker")}
        >
          <IconDatabase className="tab-icon" /> Bunker
        </button>
        <button
          className={`login-tab ${activeTab === "nsec" ? "active" : ""}`}
          onClick={() => setActiveTab("nsec")}
        >
          <IconKey className="tab-icon" /> Nsec
        </button>
      </div>

      <div className="login-content">
        {activeTab === "nostrconnect" && (
          <div className="login-method">
            <h3 className="method-title">Nostr Connect</h3>
            <p className="method-description">
              Scan with a <strong>NIP-46</strong> compatible signer app like{" "}
              <strong>Amber</strong>, <strong>Nostore</strong>, or <strong>nsec.app</strong>.
            </p>
            {connectUri ? (
              <div className="qr-container">
                <QRCodeSVG
                  value={connectUri}
                  size={200}
                  bgColor="#ffffff"
                  fgColor="#000000"
                  level="M"
                  marginSize={2}
                />
                <p className="qr-waiting">Waiting for connection...</p>
              </div>
            ) : (
              <div className="qr-container">
                <p className="qr-waiting">Generating...</p>
              </div>
            )}
          </div>
        )}

        {activeTab === "extension" && (
          <div className="login-method">
            <h3 className="method-title">Browser Extension</h3>
            <p className="method-description">
              Sign in securely using <strong>Alby</strong>, <strong>nos2x</strong>, or <strong>Flamingo</strong>.
              Your keys are never shared with this application.
            </p>
            <button
              className="login-button primary"
              onClick={handleExtensionLogin}
              disabled={isLoading}
            >
              {isLoading ? "Connecting..." : "Login with Extension"}
            </button>
          </div>
        )}

        {activeTab === "nsec" && (
          <div className="login-method">
            <h3 className="method-title">Secret Key</h3>
            <div className="method-warning">
              <IconAlertTriangle className="warning-icon" />
              <p>Entering your <strong>nsec</strong> is less secure. Use an extension or bunker if possible.</p>
            </div>
            <div className="input-group">
              <label>Your nsec key</label>
              <input
                type="password"
                value={nsec}
                onChange={(e) => setNsec(e.target.value)}
                placeholder="nsec1..."
                className="login-input"
              />
            </div>
            <div className="file-upload-section">
              <input
                type="file"
                accept=".txt"
                className="hidden"
                ref={fileInputRef}
                onChange={handleFileUpload}
              />
              <button
                className="upload-link"
                onClick={() => fileInputRef.current?.click()}
              >
                <IconFileText width={16} height={16} /> Import from .txt file
              </button>
            </div>
            <button
              className="login-button primary"
              onClick={handleNsecLogin}
              disabled={isLoading || !nsec.trim()}
            >
              {isLoading ? "Verifying..." : "Login with Nsec"}
            </button>
          </div>
        )}

        {activeTab === "bunker" && (
          <div className="login-method">
            <h3 className="method-title">Bunker URI</h3>
            <p className="method-description">
              Paste a <strong>bunker://</strong> connection string from your remote signer.
            </p>
            <div className="input-group">
              <label>Bunker URI</label>
              <input
                type="text"
                value={bunkerUri}
                onChange={(e) => setBunkerUri(e.target.value)}
                placeholder="bunker://..."
                className="login-input"
              />
              {bunkerUri && !bunkerUri.startsWith("bunker://") && (
                <p className="input-error">Invalid bunker format</p>
              )}
            </div>
            <button
              className="login-button primary"
              onClick={handleBunkerLogin}
              disabled={
                isLoading ||
                !bunkerUri.trim()
              }
            >
              {isLoading ? "Connecting..." : "Connect to Bunker"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
