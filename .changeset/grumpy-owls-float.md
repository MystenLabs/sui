---
"@mysten/sui.js": minor
---

The `TransactionBlock` builder now uses the protocol config from the chain when constructing and validating transactions, instead of using hard-coded limits. If you wish to perform signing offline (without a provider), you can either define a `protocolConfig` option when building a transaction, or explicitly set `limits`, which will be used instead of the protocol config.
