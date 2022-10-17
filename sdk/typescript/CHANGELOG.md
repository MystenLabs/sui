# @mysten/sui.js

## 0.13.0

### Minor Changes

- 1d036d459: Transactions query pagination and merge all getTransactions\* methods
- b11b69262: Add gas selection to LocalTxnSerializer
- b11b69262: Deprecate Gateway related APIs
- b11b69262: Add rpcAPIVersion to JsonRpcProvider to support multiple RPC API Versions

## 0.12.0

### Minor Changes

- e0b173b9e: Standardize Ed25519KeyPair key derivation with SLIP10
- 059ede517: Flip the default value of `skipDataValidation` to true in order to mitigate the impact of breaking changes on applications. When there's a mismatch between the Typescript definitions and RPC response, the SDK now log a console warning instead of throwing an error.
- 03e6b552b: Add util function to get coin balances
- 4575c0a02: Fix type definition of SuiMoveNormalizedType
- ccf7f148d: Added generic signAndExecuteTransaction method to the SDK, which can be used with any supported type of transaction.

### Patch Changes

- e0b173b9e: Support Pay Transaction type in local transaction serializer

## 0.11.0

### Minor Changes

- d343b67e: Re-release packages

### Patch Changes

- Updated dependencies [d343b67e]
  - @mysten/bcs@0.3.0

## 0.11.0-pre

### Minor Changes

- 5de312c9: Add support for subscribing to events on RPC using "subscribeEvent".
- 5de312c9: Add support for Secp256k1 keypairs.

### Patch Changes

- c5e4851b: Updated build process from TSDX to tsup.
- a0fdb52e: Updated publish transactions to accept ArrayLike instead of Iterable.
- e2aa08e9: Fix missing built files for packages.
- Updated dependencies [c5e4851b]
- Updated dependencies [e2aa08e9]
  - @mysten/bcs@0.2.1
