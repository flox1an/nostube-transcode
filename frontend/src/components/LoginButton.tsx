import { hasExtension, login } from "../nostr/client";

interface LoginButtonProps {
  onLogin: (pubkey: string) => void;
  onError: (error: string) => void;
}

export function LoginButton({ onLogin, onError }: LoginButtonProps) {
  const handleClick = async () => {
    if (!hasExtension()) {
      onError("No Nostr extension found. Install Alby or nos2x.");
      return;
    }

    try {
      const pubkey = await login();
      onLogin(pubkey);
    } catch (err) {
      onError(err instanceof Error ? err.message : "Login failed");
    }
  };

  return (
    <button className="login-button" onClick={handleClick}>
      Connect Wallet
    </button>
  );
}
