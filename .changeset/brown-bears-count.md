---
"@mysten/sui.js": minor
---

- removes `transfer` function from framework Coin
- renames `newTransferTx` function from framework Coin to `newPayTransaction`. Also it's now a public method and without the need of signer so a dapp can use it
- fixes edge cases with pay txs
