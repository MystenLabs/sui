---
"@mysten/wallet-adapter-unsafe-burner": minor
"@mysten/wallet-standard": minor
---

Add an optional `contentOptions` field to `SuiSignAndExecuteTransactionOptions` to specify which fields to include in `SuiTransactionBlockResponse` (e.g., transaction, effects, events, etc). By default, only the transaction digest will be included.
