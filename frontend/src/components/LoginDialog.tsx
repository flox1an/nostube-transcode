import { useState, useRef } from "react";
import { nip19 } from "nostr-tools";
import { useCurrentUser } from "../hooks/useCurrentUser";

type LoginMethod = "extension" | "nsec" | "bunker";

interface LoginDialogProps {
  onLogin: () => void;
  onError: (error: string) => void;
}

export function LoginDialog({ onLogin, onError }: LoginDialogProps) {
  const { loginWithExtension, loginWithNsec, loginWithBunker } = useCurrentUser();
  const [activeTab, setActiveTab] = useState<LoginMethod>("extension");
  const [isLoading, setIsLoading] = useState(false);
  const [nsec, setNsec] = useState("");
  const [bunkerUri, setBunkerUri] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);

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

    // Validate nsec format
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
      setNsec(""); // Clear for security
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
          className={`login-tab ${activeTab === "extension" ? "active" : ""}`}
          onClick={() => setActiveTab("extension")}
        >
          Extension
        </button>
        <button
          className={`login-tab ${activeTab === "nsec" ? "active" : ""}`}
          onClick={() => setActiveTab("nsec")}
        >
          Nsec
        </button>
        <button
          className={`login-tab ${activeTab === "bunker" ? "active" : ""}`}
          onClick={() => setActiveTab("bunker")}
        >
          Bunker
        </button>
      </div>

      <div className="login-content">
        {activeTab === "extension" && (
          <div className="login-method">
            <div className="method-icon">🔐</div>
            <p className="method-description">
              Sign in securely using a browser extension like Alby or nos2x.
              Your keys never leave your extension.
            </p>
            <button
              className="login-button"
              onClick={handleExtensionLogin}
              disabled={isLoading}
            >
              {isLoading ? "Connecting..." : "Login with Extension"}
            </button>
          </div>
        )}

        {activeTab === "nsec" && (
          <div className="login-method">
            <div className="method-icon">🔑</div>
            <p className="method-description">
              Enter your Nostr secret key (nsec). Warning: This stores your key
              in memory during the session.
            </p>
            <div className="input-group">
              <input
                type="password"
                value={nsec}
                onChange={(e) => setNsec(e.target.value)}
                placeholder="nsec1..."
                className="login-input"
              />
            </div>
            <div className="file-upload-section">
              <p className="upload-text">Or upload a key file:</p>
              <input
                type="file"
                accept=".txt"
                className="hidden"
                ref={fileInputRef}
                onChange={handleFileUpload}
              />
              <button
                className="upload-button"
                onClick={() => fileInputRef.current?.click()}
              >
                Upload nsec file
              </button>
            </div>
            <button
              className="login-button"
              onClick={handleNsecLogin}
              disabled={isLoading || !nsec.trim()}
            >
              {isLoading ? "Verifying..." : "Login with Nsec"}
            </button>
          </div>
        )}

        {activeTab === "bunker" && (
          <div className="login-method">
            <div className="method-icon">🏰</div>
            <p className="method-description">
              Connect to a remote signer using NIP-46 bunker protocol. Paste
              your bunker:// URI below.
            </p>
            <div className="input-group">
              <input
                type="text"
                value={bunkerUri}
                onChange={(e) => setBunkerUri(e.target.value)}
                placeholder="bunker://..."
                className="login-input"
              />
              {bunkerUri && !bunkerUri.startsWith("bunker://") && (
                <p className="input-error">URI must start with bunker://</p>
              )}
            </div>
            <button
              className="login-button"
              onClick={handleBunkerLogin}
              disabled={
                isLoading ||
                !bunkerUri.trim() ||
                !bunkerUri.startsWith("bunker://")
              }
            >
              {isLoading ? "Connecting..." : "Login with Bunker"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
