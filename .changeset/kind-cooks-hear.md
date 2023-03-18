---
"@mysten/sui.js": minor
---

Change functions in json-rpc-provider.ts of ts-sdk such that: getTotalTransactionNumber, getReferenceGasPrice return a bigint, getLatestCheckpointSequenceNumber returns a string, gasPrice of devInspectTransaction is defined as a string. Introduce SuiCheckpointSequenceNumber type in sui-json-rpc-types that is a BigInt to de/serialize CheckpointSequenceNumber.
