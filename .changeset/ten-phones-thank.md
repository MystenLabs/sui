---
"@mysten/sui.js": minor
---

Rename `provider.getTransactionWithEffects` to `provider.getTransaction`. The new method takes in an additional parameter `SuiTransactionBlockResponseOptions` to configure which fields to fetch(transaction, effects, events, etc). By default, only the transaction digest will be returned.
