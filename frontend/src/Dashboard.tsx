import { useState, useCallback } from "react";
import { useCurrentUser } from "./hooks/useCurrentUser";
import { LoginDialog } from "./components/LoginDialog";
import { DvmList, type UnifiedDvm } from "./components/DvmList";
import { DvmDetailPanel } from "./components/DvmDetailPanel";
import "./Dashboard.css";

import { UserAvatar } from "./components/UserAvatar";
import { UserName } from "./components/UserName";
import { IconNostube, IconTv } from "./components/Icons";

export function Dashboard() {
  const { user, isLoggedIn, logout } = useCurrentUser();
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [selectedDvm, setSelectedDvm] = useState<UnifiedDvm | null>(null);

  const handleLogout = useCallback(() => {
    logout();
    setSelectedDvm(null);
  }, [logout]);

  const handleDvmSelect = useCallback((dvm: UnifiedDvm) => {
    setSelectedDvm(dvm);
  }, []);

  // Not logged in - show login screen
  if (!isLoggedIn || !user) {
    return (
      <div className="dashboard">
        <div className="login-screen">
          <IconNostube className="brand-icon big" />
          <h1>nostube transform</h1>
          <p>Providing video transcoding for videos on NOSTR</p>
          <LoginDialog onLogin={() => setErrorMessage(null)} onError={setErrorMessage} />
          {errorMessage && <p className="error-message">{errorMessage}</p>}
        </div>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <header className="dashboard-header">
        <div className="header-brand">
          <IconNostube className="brand-icon" />
          <div className="brand-text">
            <h1>nostube transform</h1>
            <span className="brand-subtitle">Providing video transcoding for videos on NOSTR</span>
          </div>
        </div>
        <div className="user-info">
          <div className="user-pill">
            <UserAvatar pubkey={user.pubkey} size={24} />
            <UserName pubkey={user.pubkey} className="pubkey" />
          </div>
          <button className="logout-button" onClick={handleLogout}>
            Logout
          </button>
        </div>
      </header>

      <main className="dashboard-main">
        <div className="dashboard-content">
          <aside className="dvm-sidebar">
            <DvmList
              userPubkey={user.pubkey}
              selectedDvm={selectedDvm}
              onSelect={handleDvmSelect}
            />
          </aside>
          <section className="dvm-detail-panel-container">
            {selectedDvm ? (
              <DvmDetailPanel dvm={selectedDvm} userPubkey={user.pubkey} key={selectedDvm.pubkey} />
            ) : (
              <div className="no-selection">
                <div className="no-selection-content">
                  <IconTv className="no-selection-icon" />
                  <h2>Welcome to nostube transform</h2>
                  <p>Select a DVM from the sidebar to manage it or submit a video for processing.</p>
                </div>
              </div>
            )}
          </section>
        </div>
      </main>
    </div>
  );
}
