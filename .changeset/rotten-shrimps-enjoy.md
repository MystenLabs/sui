---
"@mysten/sui.js": patch
---

Previously, effects had an unwrapped_then_deleted field on ts-sdk. This is an issue since jsonrpc returns the field as unwrappedThenDeleted. Update the transaction type definition to use camelcase.
