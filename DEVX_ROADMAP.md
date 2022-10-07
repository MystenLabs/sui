# Developer Experience Roadmap
(last updated 10/11/2022, next update approximately ~11/11/2022 following a monthly cadence)

To keep Sui builders up to date with the latest happenings, we are maintaining the following list of developer-facing changes coming in the next ~30 days. While we strive to be accurate, the timing of the landing/release of these features is subject to change. More thorough documentation and references will be available as each feature is released to Devnet. Please continue to monitor Devnet release notes for the source of truth on what is currently deployed--this list is about what's next!

## Big picture
* The gateway component will be deprecated soon and we'll give community sufficient heads up once the dates are finalized. With this deprecation, clients will now communicate directly with full nodes, and full nodes will act as quorum driver (i.e., accept transactions, form a certificate + certificate effects, return the effects to the client).
* Child objects revamp in still WIP. The features eliminates the need to explicitly pass transaction inputs, and enables rich new collection types (e.g., large hash maps with dynamic lookup).
* We are adding new pay transaction types for generic payments. See below for more details.

## JSON RPC
* [Gateway deprecation] Working on adding metrics, logging and thorough testing of fullnodes. Also, working on a migration guide to enable smooth transition.
* `getObject` and `getRawObject` will be merged, and `DataEncoding` arg will be used to choose between parsedJSON and BCS encoding types.
* Adding `getCoin` and `getBalance` methods
* Adding `pay`, which will take multiple coins, splits & merges them within the same transaction when needed, and transfers the resulting coins to multiple recipients. Deprecating `splitCoin`, `splitCoinEqual` and `mergeCoin` in that process, as they can all be done via the `pay` endpoint [**Breaking Change**].
* Adding `paySui`, which takes multiple coins and transfer them to a single recipient, while paying gas with the input coins. Deprecating `transferSui` as `paySui` is a generalized version of it [**Breaking Change**].
* Adding `estimateTransactionComputationCost` for transaction cost estimation.
* Adding `selectCoins` to select coins for gas payments, pay etc.
* Standardizing the return type of u64 to string.

## SDK (Typescript, Rust)

* Intent signing support which will include an intent struct to be serialized and signed in addition to the transaction data [[Issues](https://github.com/MystenLabs/fastcrypto/issues/26)].
* [Rust SDK] Removing reliance on String and Json Values in Rust Transaction Builder, making it more Rust friendly
* Adding denomination conversion functionality, which converts SUI to MIST.
* [TypeScript] Adding support for deserializing transactions.

## Sui Move

* Crypto.move will be split into its individual modules organized by crypto primitives [[PR](https://github.com/MystenLabs/sui/pull/4653)].
* Supporting dynamic child access requires additional time due to newer complexities discovered and we are actively working on enabling it on devNet ASAP. 
* Support for passing mutable coins instead of owned.

## Sui CLI
* Intent signing support: Same as in SDK. Also, making the Sui keystore ChainID aware as well as password protected.

## Faucet
* As a precursor to integrating Faucet into Sui CLI/SDKs/Wallet, rate-limiting will be enforced on the faucet server to 5 requests per minute and no more than 20 requests in 24 hours, originating from the same ip address.
* Adding support for requesting Gas coins from the faucet.
