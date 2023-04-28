# @mysten/sui.js

## 0.32.2

### Patch Changes

- 4ae3cbea3: Response for `getCoinMetadata` is now nullable, in the event that no metadata can be found.
- d2755a496: Fix dependency on msw
- f612dac98: Change the default gas budgeting to take storage rebates into account.
- c219e7470: Changed the response type of `getRpcApiVersion` to string.
- 59ae0e7d6: Removed `skipDataValidation` option, this is now not configurable and is the default behavior.
- c219e7470: Fix type of `limit` on `getCheckpoints` and `getEpochs` API so that is correctly a number.
- 4e463c691: Add `waitForTransactionBlock` API to wait for a transaction to be available over the API.
- b4f0bfc76: Fix type definitions for package exports.
- Updated dependencies [b4f0bfc76]
  - @mysten/bcs@0.7.1

## 0.32.1

### Patch Changes

- 3224ffcd0: Adding support for the `upgrade` transaction type.

## 0.32.0

### Minor Changes

- 9b42d0ada: This release replaces all uint64 and uint128 numbers with BigInt in all JSON RPC responses to preserve precision. This is a Major Breaking Change - you must update your TS-SDK to latest version

## 0.31.0

### Minor Changes

- 976d3e1fe: Add new `getNetworkMetrics` endpoint to JSONRPCProvider.
- 5a4e3e416: Change getOwnedObject to ignore checkpoint and return latest objects

### Patch Changes

- 0419b7c53: Match ts Publish schema to rust sdk
- f3c096e3a: Fix PaginatedObjectsResponse schema
- 27dec39eb: Make getOwnedObjects backward compatible from 0.29 to 0.30.

## 0.30.0

### Minor Changes

- 956ec28eb: Change `signMessage` to return message bytes. Add support for sui:signMessage in the wallet standard
- 4adfbff73: Use Blake2b instead of sha3_256 for address generation
- 4c4573ebe: Removed DevInspectResultsType and now DevInspectResults has a property results of ExecutionResultType and a property error
- acc2edb31: Update schema for `SuiSystemState` and `DelegatedStake`
- 941b03af1: Change functions in transactions.ts of ts-sdk such that: `getTotalGasUsed` and `getTotalGasUsedUpperBound` of ts-sdk return a `bigint`,fields of `gasCostSummary` are defined as `string`, `epochId` is defined as `string`. In `sui-json-rpc` the corresponding types are defined as `BigInt`. Introduce `SuiEpochId` type to `sui-json-rpc` types that is a `BigInt`.
- a6690ac7d: Changed the default behavior of `publish` to publish an upgreadeable-by-sender package instead of immutable.
- a211dc03a: Change object digest from Base64 encoded to Base58 encoded for rpc version >= 0.28.0
- 4c1e331b8: Gas budget is now optional, and will automatically be computed by executing a dry-run when not provided.
- 19b567f21: Unified self- and delegated staking flows. Removed fields from `Validator` (`stake_amount`, `pending_stake`, and `pending_withdraw`) and renamed `delegation_staking_pool` to `staking_pool`. Additionally removed the `validator_stake` and `delegated_stake` fields in the `ValidatorSet` type and replaced them with a `total_stake` field.
- 7659e2e91: Introduce new `Transaction` builder class, and deprecate all existing methods of sending transactions. The new builder class is designed to take full advantage of Programmable Transactions. Any transaction using the previous `SignableTransaction` interface will be converted to a `Transaction` class when possible, but this interface will be fully removed soon.
- 0d3cb44d9: Change all snake_case field in ts-sdk normalized.ts to camelCase.
- 36c264ebb: Remove `generateTransactionDigest`. Use one of the following instead: `signer.getTransactionDigest`, `Transaction.getDigest()` or `TransactionDataBuilder.getDigestFromBytes()` instead.
- 891abf5ed: Remove support for RPC Batch Request in favor of multiGetTransactions and multiGetObjects
- 2e0ef59fa: Added VALIDATORS_EVENTS_QUERY
- 33cb357e1: Change functions in json-rpc-provider.ts of ts-sdk such that: `getTotalTransactionBlocks`, `getReferenceGasPrice` return a `bigint`, `getLatestCheckpointSequenceNumber` returns a `string`, `gasPrice` of `devInspectTransactionBlock` is defined as a `string`, checkpoint sequence number of `getCheckpoint` is defined as a `string`, `cursor` of `getCheckpoints` is defined as a `string`. Introduce `SuiCheckpointSequenceNumber` type in sui-json-rpc-types that is a `BigInt` to use instead of `CheckpointSequenceNumber` of sui-types.
- 6bd88570c: Rework all coin APIs to take objects as arguments instead of positional arguments.
- f1e42f792: Consolidate get_object and get_raw_object into a single get_object endpoint which now takes an additional config parameter with type `SuiObjectDataOptions` and has a new return type `SuiObjectResponse`. By default, only object_id, version, and digest are fetched.
- 272389c20: Support for new versioned TransactionData format
- 3de8de361: Remove `getSuiSystemState` method. Use `getLatestSuiSystemState` method instead.
- be3c4f51e: Add `display` field in `SuiObjectResponse` for frontend rendering. See more details in https://forums.sui.io/t/nft-object-display-proposal/4872
- dbe73d5a4: Update `executeTransaction` and `signAndExecuteTransaction` to take in an additional parameter `SuiTransactionBlockResponseOptions` which is used to specify which fields to include in `SuiTransactionBlockResponse` (e.g., transaction, effects, events, etc). By default, only the transaction digest will be included.
- c82e4b454: Introduce BigInt struct to sui-json-rpc-types to serialize and deserialize amounts to/from string. Change ts-sdk to serialize amounts of PaySui and Pay as string.
- 7a2eaf4a3: Changing the SuiObjectResponse struct to use data/error fields instead of details/status
- 2ef2bb59e: Deprecate getTransactionDigestsInRange. This method will be removed before April 2023, please use `getTransactions` instead
- 9b29bef37: Pass blake2b hash to signer API
- 8700809b5: Add a new `getCheckpoints` endpoint that returns a paginated list of checkpoints.
- 5c3b00cde: Add object id to staking pool and pool id to staked sui.
- 01272ab7d: Remove deprecated `getCheckpointContents`, `getCheckpointContentsByDigest`, `getCheckpointSummary` and `getCheckpointSummaryByDigest` methods.
- 9822357d6: Add getStakesByIds to get DelegatedStake queried by id
- 3d9a04648: Adds `deactivation_epoch` to staking pool object, and adds `inactive_pools` to the validator set object.
- da72e73a9: Change the address of Move package for staking and validator related Move modules.
- a0955c479: Switch from 20 to 32-byte address. Match Secp256k1.deriveKeypair with Ed25519.
- 0c9047698: Remove all gas selection APIs from the json rpc provider.
- d5ef1b6e5: Added dependencies to publish command, dependencies now also returned from the sui move CLI with the `--dump-bytecode-as-base64` flag
- 0a7b42a6d: This changes almost all occurences of "delegate", "delegation" (and various capitalizations/forms) to their equivalent "stake"-based name. Function names, function argument names, RPC endpoints, Move functions, and object fields have been updated with this new naming convention.
- 3de8de361: Remove `getValidators` API. Use `getLatestSuiSystemState` instead.
- dd348cf03: Refactor `getTransactions` to `queryTransactions`
- 57c17e02a: Removed `JsonRpcProviderWithCache`, use `JsonRpcProvider` instead.
- 65f1372dd: Rename `provider.getTransactionWithEffects` to `provider.getTransaction`. The new method takes in an additional parameter `SuiTransactionBlockResponseOptions` to configure which fields to fetch(transaction, effects, events, etc). By default, only the transaction digest will be returned.
- a09239308: [testing only] an intent scope can be passed in to verifyMessage
- fe335e6ba: Removed usage of `cross-fetch` in the TypeScript SDK. If you are running in an environment that does not have `fetch` defined, you will need to polyfill it.
- 5dc25faad: Remove getTransactionDigestsInRange from the SDK
- 64234baaf: added combined `getCheckpoint` endpoint for retrieving information about a checkpoint
- d3170ba41: All JSON-RPC APIs now accept objects instead of positional arugments.
- a6ffb8088: Removed events from transaction effects, TransactionEvents will now be provided in the TransactionResponse, along side TransactionEffects.
- 3304eb83b: Refactor Rust SuiTransactionBlockKind to be internally tagged for Json serialization with tag="type" and SuiEvent to be adjacently tagged with tag="type" and content="content"
- 4189171ef: Adds support for validator candidate.
- 77bdf907f: When parsing u64, u128, and u256 values with bcs, they are now string encoded.
- a74df16ec: Minor change to the system transaction format
- 0f7aa6507: Switching the response type of the getOwnedObjects api to a paginatedObjects response, and also moving filtering to FN
- 9b60bf700: Change all snake_case fields in checkpoint.ts and faucet.ts to camelCase
- 64fb649eb: Remove old `SuiExecuteTransactionResponse` interface, and `CertifiedTransaction` interface in favor of the new unified `SuiTransactionBlockResponse` interfaces.
- a6b0c4e5f: Changed the getOwnerObjectsForAddress api to getOwnedObjects, and added options/ pagination to the parameters

### Patch Changes

- 00bb9bb66: Correct "consensus_address" in ValidatorMetadata to "primary_address"
- 14ba89144: Change StakingPool structure by removing pool token supply and adding exchange rates.
- 3eb3a1de8: Make Ed25519 ExportedKeyPair only use 32 bytes seed.
- 4593333bd: Add optional parameter for filtering object by type in getOwnedObjectsByAddress
- 79c2165cb: Remove locked coin staking
- 210840114: Add cross-env to prepare:e2e script for Windows machines functionality
- Updated dependencies [19b567f21]
- Updated dependencies [5c3b00cde]
- Updated dependencies [3d9a04648]
- Updated dependencies [a8049d159]
- Updated dependencies [a0955c479]
- Updated dependencies [0a7b42a6d]
- Updated dependencies [77bdf907f]
  - @mysten/bcs@0.7.0

## 0.29.1

### Patch Changes

- 31bfcae6a: Make arguments field optional for MoveCall to match Rust definition. This fixes a bug where the Explorer page does not load for transactions with no argument.

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
