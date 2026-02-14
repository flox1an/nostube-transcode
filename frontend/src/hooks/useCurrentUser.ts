import { useMemo } from 'react'
import { useActiveAccount, useAccountManager } from 'applesauce-react/hooks'
import { ExtensionAccount, SimpleAccount, NostrConnectAccount } from 'applesauce-accounts/accounts'
import { ExtensionSigner, SimpleSigner, NostrConnectSigner } from 'applesauce-signers/signers'

export function useCurrentUser() {
  const accountManager = useAccountManager(false)
  const activeAccount = useActiveAccount()

  const isLoggedIn = !!activeAccount
  const user = useMemo(
    () =>
      activeAccount
        ? {
            pubkey: activeAccount.pubkey,
            signer: activeAccount.signer,
          }
        : undefined,
    [activeAccount]
  )

  const loginWithExtension = async () => {
    if (!accountManager) throw new Error('Account manager not available')

    const signer = new ExtensionSigner()
    const pubkey = await signer.getPublicKey()
    const account = new ExtensionAccount(pubkey, signer)

    await accountManager.addAccount(account)
    accountManager.setActive(account)
  }

  const loginWithNsec = async (nsec: string) => {
    if (!accountManager) throw new Error('Account manager not available')

    const signer = SimpleSigner.fromKey(nsec)
    const pubkey = await signer.getPublicKey()
    const account = new SimpleAccount(pubkey, signer)

    await accountManager.addAccount(account)
    accountManager.setActive(account)
  }

  const loginWithBunker = async (bunkerUri: string) => {
    if (!accountManager) throw new Error('Account manager not available')

    const signer = await NostrConnectSigner.fromBunkerURI(bunkerUri)
    const pubkey = await signer.getPublicKey()
    const account = new NostrConnectAccount(pubkey, signer)

    await accountManager.addAccount(account)
    accountManager.setActive(account)
  }

  const logout = () => {
    if (activeAccount && accountManager) {
      // @ts-ignore
      accountManager.clearActive();
      accountManager.removeAccount(activeAccount.pubkey)
    }
  }

  return {
    user,
    isLoggedIn,
    loginWithExtension,
    loginWithNsec,
    loginWithBunker,
    logout,
  }
}
