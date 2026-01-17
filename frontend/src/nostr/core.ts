import { EventStore } from 'applesauce-core'
import { RelayPool } from 'applesauce-relay'
import { NostrConnectSigner } from 'applesauce-signers'
import type { NostrSubscriptionMethod, NostrPublishMethod } from 'applesauce-signers'
import type { Filter, NostrEvent } from 'nostr-tools'
import { filter } from 'rxjs'
import { RELAYS } from './constants'

export const eventStore = new EventStore()
export const relayPool = new RelayPool()

// Default relays for the app
export const defaultRelays = RELAYS

export const subscriptionMethod: NostrSubscriptionMethod = (
  relays: string[],
  filters: Filter[]
) => {
  return relayPool
    .subscription(relays, filters)
    .pipe(
      filter(
        (response): response is NostrEvent => typeof response !== 'string' && 'kind' in response
      )
    )
}

export const publishMethod: NostrPublishMethod = async (relays: string[], event: NostrEvent) => {
  return await relayPool.publish(relays, event)
}

// Configure NostrConnectSigner to use our relay pool
NostrConnectSigner.subscriptionMethod = subscriptionMethod
NostrConnectSigner.publishMethod = publishMethod
