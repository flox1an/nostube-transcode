import { useCurrentUser } from "../hooks/useCurrentUser";

interface LoginButtonProps {
  onLogin: () => void;
  onError: (error: string) => void;
}

export function LoginButton({ onLogin, onError }: LoginButtonProps) {
  const { loginWithExtension } = useCurrentUser();

  const hasExtension = () => {
    return typeof window !== "undefined" && "nostr" in window;
  };

  const handleClick = async () => {
    if (!hasExtension()) {
      onError("No Nostr extension found. Install Alby or nos2x.");
      return;
    }

    try {
      await loginWithExtension();
      onLogin();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Login failed");
    }
  };

  return (
    <button className="login-button" onClick={handleClick}>
      Login with Extension
    </button>
  );
}
