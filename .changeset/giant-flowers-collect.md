---
"@mysten/wallet-kit-core": patch
---

- delay auto connect until document is visible - fix preloading dapp issues
  - fixes showing the wallet connect popup (for cases wallet was disconnected without dapp to be notified) when preloading the page (usually while typing the url)
  - prevents content script from creating a Port to service worker while the dapp is hidden, which causes the port to be in a disconnected state in SW but without notifying the CS, when page is preloaded.
