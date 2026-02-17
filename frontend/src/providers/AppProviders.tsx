import { type ReactNode } from 'react'
import {
  AccountsProvider,
  EventStoreProvider,
} from 'applesauce-react/providers'
import { AccountManager } from 'applesauce-accounts'
import { registerCommonAccountTypes } from 'applesauce-accounts/accounts'
import { eventStore } from '../nostr/core'

const ACCOUNTS_KEY = 'divico:accounts'
const ACTIVE_KEY = 'divico:active'

const accountManager = new AccountManager()
registerCommonAccountTypes(accountManager)

// Restore persisted accounts
try {
  const raw = localStorage.getItem(ACCOUNTS_KEY)
  if (raw) {
    accountManager.fromJSON(JSON.parse(raw))
    const activeId = localStorage.getItem(ACTIVE_KEY)
    if (activeId && accountManager.getAccount(activeId)) {
      accountManager.setActive(activeId)
    }
  }
} catch {
  localStorage.removeItem(ACCOUNTS_KEY)
  localStorage.removeItem(ACTIVE_KEY)
}

// Persist on every change
accountManager.accounts$.subscribe(() => {
  try {
    const json = accountManager.toJSON()
    if (json.length > 0) {
      localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(json))
    } else {
      localStorage.removeItem(ACCOUNTS_KEY)
    }
  } catch {
    // ignore serialization errors
  }
})

accountManager.active$.subscribe((account) => {
  if (account) {
    localStorage.setItem(ACTIVE_KEY, account.id)
  } else {
    localStorage.removeItem(ACTIVE_KEY)
  }
})

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
