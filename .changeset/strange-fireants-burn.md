---
"@mysten/wallet-adapter-wallet-standard": minor
"@mysten/wallet-adapter-unsafe-burner": minor
"@mysten/wallet-adapter-base": minor
"@mysten/wallet-kit-core": minor
"@mysten/wallet-standard": minor
"@mysten/wallet-kit": minor
---

wallet-standard: changes sui:signAndExecuteTransaction and sui:signTransaction features to support account and chain options
wallet-adapter-wallet-standard: change signAndExecuteTransaction and signTransaction signatures to support account and chain options
wallet-adapter-wallet-standard: ensure version compatibility for of the wallet signAndExecuteTransaction and signTransaction features before using them (same major version)
wallet-kit-core/wallet-kit: expose accounts as ReadonlyWalletAccount instead of only the address
wallet-kit-core: signTransaction and signAndExecuteTransaction methods mirror the ones in standard adapter
