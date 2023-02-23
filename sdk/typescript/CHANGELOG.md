# @mysten/sui.js

## 0.29.0

### Minor Changes

- f2e713bd0: Add TransactionExpiration to TransactionData
- 4baf554f1: Make fromSecretKey take the 32 bytes privkey
- aa650aa3b: Introduce new `Connection` class, which is used to define the endpoints that are used when interacting with the network.
- 6ff0c785f: Use DynamicFieldName struct instead of string for dynamic field's name

### Patch Changes

- f1e3a0373: Expose rpcClient and websocketClient options
- 0e202a543: Remove pending delegation switches.
- 67e503c7c: Move base58 libraries to BCS
- Updated dependencies [0e202a543]
  - @mysten/bcs@0.6.1

## 0.28.0

### Minor Changes

- a67cc044b: Transaction signatures are now serialized into a single string, and all APIs that previously took the public key, signature, and scheme now just take the single serialized signature string. To help make parsing this easier, there are new `toSerializedSignature` and `fromSerializedSignature` methods exposed as well.
- a67cc044b: The RawSigner now provides a `signTransaction` function, which can be used to sign a transaction without submitting it to the network.
- a67cc044b: The RawSigner now provides a `signMessage` function that can be used to sign personal messages. The SDK also now exports a `verifyMessage` function that can be used to easily verify a message signed with `signMessage`.

### Patch Changes

- 24bdb66c6: Include client type and version in RPC client request headers
- Updated dependencies [598f106ef]
  - @mysten/bcs@0.6.0

## 0.27.0

### Minor Changes

- 473005d8f: Add protocol_version to CheckpointSummary and SuiSystemObject. Consolidate end-of-epoch information in CheckpointSummary.
- 59641dc29: Support for deserializing new ConsensusCommitPrologue system transaction
- 629804d26: Remove usage of `Base64DataBuffer`, and use `Uint8Array` instead.
- f51c85e85: remove get_objects_owned_by_object and replace it with get_dynamic_fields

### Patch Changes

- fcba70206: Add basic formatting utilities
- ebe6c3945: Support deserializing `paySui` and `payAllSui` transactions
- e630f6832: Added string option to getCheckpointContents call in SDK to support 0.22.0

## 0.26.1

### Patch Changes

- 97c46ca9d: Support calling Move function with "option" parameter

## 0.26.0

### Minor Changes

- a8746d4e9: update SuiExecuteTransactionResponse
- e6a71882f: Rename getDelegatedStake to getDelegatedStakes
- 21781ba52: Secp256k1 signs 64-bytes signature [r, s] instead of [r, s, v] with recovery id

### Patch Changes

- 034158656: Allow passing Pure args directly in Move call
- 57fc4dedd: Fix gas selection logic to take gas price into account
- e6a71882f: Add convenience methods in RpcTxnDataSerializer for building staking transactions
- b3ba6dfbc: Support Genesis transaction kind

## 0.25.0

### Minor Changes

- 7b4bf43bc: Support for interacting with Devnet v0.24+ where Move Calls refer to their packages by ObjectID only (not ObjectRef).

### Patch Changes

- ebfdd5c56: Adding Checkpoint APIs for ts sdk
- 72481e759: Updated to new dev inspect transaction layout
- 969a88669: RPC requests errors now don't include the html response text (to keep message shorter)

## 0.24.0

### Minor Changes

- 88a687834: Add methods for the CoinRead endpoints

### Patch Changes

- 01458ffd5: Fix websocket default port for DevNet
- a274ecfc7: Make previousTransaction optional for CoinStruct to support v0.22 network where it doesn't exist
- 89091ddab: change estimator logic to use upper bound
- 71bee7563: fix creating websocket url

## 0.23.0

### Minor Changes

- e26f47cbf: added getDelegatedStake and getValidators and validator type
- b745cde24: Add a call(endpoint, params) method to invoke any RPC endpoint
- 35e0df780: EventID should use TransactionDigest instead of TxSequence
- 5cd51dd38: Deprecate sui_executeTransaction in favor of sui_executeTransactionSerializedSig
- 8474242af: Add methods for getDynamicFields and getDynamicFieldObject
- f74181212: Add method to deserialize a public key, using it's schema and base64 data

### Patch Changes

- f3444bdf2: fix faucet response type
- 01efa8bc6: Add getReferenceGasPrice
- 01efa8bc6: Use reference gas price instead of a hardcoded "1" for transaction construction

## 0.22.0

### Minor Changes

- a55236e48: Add gas price field to RPC transaction data type

### Patch Changes

- 8ae226dae: Fix schema validation bug in Coin.newPayTransaction

## 0.21.0

### Minor Changes

- 4fb12ac6d: - removes `transfer` function from framework Coin
  - renames `newTransferTx` function from framework Coin to `newPayTransaction`. Also it's now a public method and without the need of signer so a dapp can use it
  - fixes edge cases with pay txs
- bb14ffdc5: Remove ImmediateReturn and WaitForTxCert from ExecuteTransactionRequestType
- d2015f815: Rebuilt type-narrowing utilties (e.g. `isSuiObject`) on top of Superstruct, which should make them more reliable.
  The type-narrowing functions are no longer exported, instead a Superstruct schema is exported, in addition to an `is` and `assert` function, both of which can be used to replace the previous narrowing functions. For example, `isSuiObject(data)` becomes `is(data, SuiObject)`.
- 7d0f25b61: Add devInspectTransaction, which is similar to dryRunTransaction, but lets you call any Move function(including non-entry function) with arbitrary values.

### Patch Changes

- 9fbe2714b: Add devInspectMoveCall, which is similar to devInspectTransaction, but lets you call any Move function without a gas object and budget

## 0.20.0

### Minor Changes

- ea71d8216: Use intent signing if sui version > 0.18

### Patch Changes

- f93b59f3a: Fixed usage of named export for CommonJS module

## 0.19.0

### Minor Changes

- 6c1f81228: Remove signature from trasaction digest hash
- 519e11551: Allow keypairs to be exported
- b03bfaec2: Add getTransactionAuthSigners endpoint

### Patch Changes

- b8257cecb: add missing int types
- f9be28a42: Fix bug in Coin.isCoin
- 24987df35: Regex change for account index for supporting multiple accounts

## 0.18.0

### Minor Changes

- 66021884e: Send serialized signature with new executeTransactionSerializedSig endpoint
- 7a67d61e2: Unify TxnSerializer interface
- 2a0b8e85d: Add base58 encoding for TransactionDigest

### Patch Changes

- 45293b6ff: Replace `getCoinDenominationInfo` with `getCoinMetadata`
- 7a67d61e2: Add method in SignerWithProvider for calculating transaction digest

## 0.17.1

### Patch Changes

- 623505886: Fix callArg serialization bug in LocalTxnSerializer

## 0.17.0

### Minor Changes

- a9602e533: Remove deprecated events API
- db22728c1: \* adds dryRunTransaction support
  - adds getGasCostEstimation to the signer-with-provider that estimates the gas cost for a transaction
- 3b510d0fc: adds coin transfer method to framework that uses pay and paySui

## 0.16.0

### Minor Changes

- 01989d3d5: Remove usage of Buffer within SDK
- 5e20e6569: Event query pagination and merge all getEvents\* methods

### Patch Changes

- Updated dependencies [1a0968636]
  - @mysten/bcs@0.5.0

## 0.15.0

### Minor Changes

- c27933292: Update the type of the `endpoint` field in JsonRpcProvider from string to object

### Patch Changes

- c27933292: Add util function for faucet
- 90898d366: Support passing utf8 and ascii string
- c27933292: Add constants for default API endpoints
- Updated dependencies [1591726e8]
- Updated dependencies [1591726e8]
  - @mysten/bcs@0.4.0

## 0.14.0

### Minor Changes

- 8b4bea5e2: Remove gateway related APIs
- e45b188a8: Introduce PaySui and PayAllSui native transaction types to TS SDK.

### Patch Changes

- e86f8bc5e: Add `getRpcApiVersion` to Provider interface
- b4a8ee9bf: Support passing a vector of objects in LocalTxnBuilder
- ef3571dc8: Fix gas selection bug for a vector of objects
- cccfe9315: Add deserialization util method to LocalTxnDataSerializer
- 2dc594ef7: Introduce getCoinDenominationInfo, which returns denomination info of a coin, now only supporting SUI coin.
- 4f0c611ff: Protocol change to add 'initial shared version' to shared object references.

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
