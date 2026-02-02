import { useState, useCallback } from "react";
import { useCurrentUser } from "./hooks/useCurrentUser";
import { LoginDialog } from "./components/LoginDialog";
import { DvmList, type UnifiedDvm } from "./components/DvmList";
import { DvmDetailPanel } from "./components/DvmDetailPanel";
import { PairDvmModal } from "./components/PairDvmModal";
import "./Dashboard.css";

export function Dashboard() {
  const { user, isLoggedIn, logout } = useCurrentUser();
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [selectedDvm, setSelectedDvm] = useState<UnifiedDvm | null>(null);
  const [showPairModal, setShowPairModal] = useState(false);

  const handleLogout = useCallback(() => {
    logout();
    setSelectedDvm(null);
  }, [logout]);

  const handleDvmSelect = useCallback((dvm: UnifiedDvm) => {
    setSelectedDvm(dvm);
  }, []);

  const handlePairNew = useCallback(() => {
    setShowPairModal(true);
  }, []);

  const handlePairComplete = useCallback(() => {
    setShowPairModal(false);
  }, []);

  // Not logged in - show login screen
  if (!isLoggedIn || !user) {
    return (
      <div className="dashboard">
        <div className="login-screen">
          <h1>DVM Video Processor</h1>
          <p>Login to discover and manage DVMs</p>
          <LoginDialog onLogin={() => setErrorMessage(null)} onError={setErrorMessage} />
          {errorMessage && <p className="error-message">{errorMessage}</p>}
        </div>
      </div>
    );
  }

  const truncatePubkey = (pubkey: string): string => {
    return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
  };

  return (
    <div className="dashboard">
      <header className="dashboard-header">
        <h1>DVM Video Processor</h1>
        <div className="user-info">
          <span className="pubkey">{truncatePubkey(user.pubkey)}</span>
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
              onPairNew={handlePairNew}
            />
          </aside>
          <section className="dvm-detail-panel-container">
            {selectedDvm ? (
              <DvmDetailPanel dvm={selectedDvm} userPubkey={user.pubkey} />
            ) : (
              <div className="no-selection">
                <p>Select a DVM from the list to view details</p>
              </div>
            )}
          </section>
        </div>
      </main>

      {showPairModal && (
        <PairDvmModal
          onClose={() => setShowPairModal(false)}
          onSuccess={handlePairComplete}
        />
      )}
    </div>
  );
}
