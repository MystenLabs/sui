---
"@mysten/sui.js": minor
---

Change functions in transactions.ts of ts-sdk such that: `getTotalGasUsed` and `getTotalGasUsedUpperBound` of ts-sdk return a `bigint`,fields of `gasCostSummary` are defined as `string`, `epochId` is defined as `string`. In `sui-json-rpc` the corresponding types are defined as `BigInt`. Introduce `SuiEpochId` type to `sui-json-rpc` types that is a `BigInt`.
