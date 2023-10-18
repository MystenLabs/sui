---
'@mysten/dapp-kit': minor
---

Add global connection status info and change the hook interface of `useCurrentWallet` to
return an object to encapsulate connection info together. To migrate:

Before:
const currentWallet = useCurrentWallet();

After:
const { currentWallet } = useCurrentWallet();
