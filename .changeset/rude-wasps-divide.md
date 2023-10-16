---
'@mysten/dapp-kit': major
---

Add global connection status info to useCurrentWallet and change the hook interface to an object
to encapsulate this information together. To migrate, you can simply do the following:

Before:
const currentWallet = useCurrentWallet();

After:
const { currentWallet } = useCurrentWallet();
