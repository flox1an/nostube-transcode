import { useState, useCallback } from "react";
import { useCurrentUser } from "./hooks/useCurrentUser";
import { LoginDialog } from "./components/LoginDialog";
import "./Dashboard.css";

export function Dashboard() {
  const { user, isLoggedIn, logout } = useCurrentUser();
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleLogout = useCallback(() => {
    logout();
  }, [logout]);

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
            {/* DVM list with filter will go here */}
            <p>DVM list placeholder</p>
          </aside>
          <section className="dvm-detail-panel">
            {/* Selected DVM detail will go here */}
            <p>Select a DVM to view details</p>
          </section>
        </div>
      </main>
    </div>
  );
}
