import { type ReactNode } from 'react'
import {
  AccountsProvider,
  EventStoreProvider,
} from 'applesauce-react/providers'
import { AccountManager } from 'applesauce-accounts'
import { registerCommonAccountTypes } from 'applesauce-accounts/accounts'
import { eventStore } from '../nostr/core'

const accountManager = new AccountManager()
registerCommonAccountTypes(accountManager)

interface AppProvidersProps {
  children: ReactNode
}

export function AppProviders({ children }: AppProvidersProps) {
  return (
    <AccountsProvider manager={accountManager}>
      <EventStoreProvider eventStore={eventStore}>
        {children}
      </EventStoreProvider>
    </AccountsProvider>
  )
}

export { accountManager }
