import { useEffect } from "react";
import { useObservableMemo } from "applesauce-react/hooks";
import { eventStore } from "../nostr/core";
import { subscribeToMetadata } from "../nostr/client";
import "./UserAvatar.css";

interface UserAvatarProps {
  pubkey: string;
  size?: number;
  className?: string;
}

export function UserAvatar({ pubkey, size = 40, className = "" }: UserAvatarProps) {
  // EventStore.profile returns Observable<ProfileContent | undefined>
  // @ts-ignore - useObservableMemo might expect BehaviorSubject but Observable often works or can be cast
  const profile = useObservableMemo(() => eventStore.profile(pubkey), [pubkey]);

  useEffect(() => {
    if (!profile) {
      const unsubscribe = subscribeToMetadata([pubkey]);
      return () => unsubscribe();
    }
  }, [pubkey, !!profile]);

  const style = {
    width: size,
    height: size,
    minWidth: size,
    minHeight: size,
  };

  if (profile?.picture) {
    return (
      <img
        src={profile.picture}
        alt={profile.display_name || profile.name || pubkey}
        className={`user-avatar ${className}`}
        style={style}
      />
    );
  }

  return (
    <div className={`user-avatar-placeholder ${className}`} style={style}>
      { (profile?.display_name || profile?.name || "?").charAt(0).toUpperCase() }
    </div>
  );
}
