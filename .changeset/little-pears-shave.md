---
"@mysten/sui.js": minor
---

Update `executeTransaction` and `signAndExecuteTransaction` to take in an additional parameter `SuiTransactionResponseOptions` which is used to specify which fields to include in `SuiTransactionResponse` (e.g., transaction, effects, events, etc). By default, only the transaction digest will be included.
