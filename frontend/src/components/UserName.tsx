import { useEffect } from "react";
import { useObservableMemo } from "applesauce-react/hooks";
import { eventStore } from "../nostr/core";
import { subscribeToMetadata } from "../nostr/client";
import { nip19 } from "nostr-tools";

interface UserNameProps {
  pubkey: string;
  className?: string;
}

export function UserName({ pubkey, className = "" }: UserNameProps) {
  // @ts-ignore
  const profile = useObservableMemo(() => eventStore.profile(pubkey), [pubkey]);

  useEffect(() => {
    if (!profile) {
      const unsubscribe = subscribeToMetadata([pubkey]);
      return () => unsubscribe();
    }
  }, [pubkey, !!profile]);

  const truncatePubkey = (pk: string): string => {
    try {
      const npub = nip19.npubEncode(pk);
      return `${npub.slice(0, 8)}...${npub.slice(-8)}`;
    } catch {
      return `${pk.slice(0, 8)}...${pk.slice(-8)}`;
    }
  };

  const displayName = profile?.display_name || profile?.name || truncatePubkey(pubkey);

  return (
    <span className={`user-name ${className}`} title={pubkey}>
      {displayName}
    </span>
  );
}
