---
"@mysten/sui.js": minor
---

Update `executeTransaction` and `signAndExecuteTransaction` to take in an additional parameter `SuiTransactionBlockResponseOptions` which is used to specify which fields to include in `SuiTransactionBlockResponse` (e.g., transaction, effects, events, etc). By default, only the transaction digest will be included.
