# Sui Developer Experience Roadmap

Last updated: 11/29/2022
Next update:  on or about 12/29/2022

To keep Sui builders up-to-date with the latest happenings, we provide the following list of developer-facing changes planned in the next ~30 days. While we strive to be accurate, the timing to release the planned features is subject to change. We provide more thorough documentation and references for each feature release to Sui Devnet. Please continue to monitor Devnet release notes as the source of truth for features deployed to Devnet. This document informs you about the currently planned upcoming changes to Sui.

## Highlights

 * We plan to add the ability for developers to depend on packages from third-party package managers. More information in the following sections.
 * We are introducing Lamport Object Versions, a new mechanism to version Objects. More details below.
 * We continue to make stability and performance improvements to our JSON RPC APIs.
 * We intend to introduce signing support soon.


## JSON RPC

 * Remove the requirement for a user signature from `sui_dryRunTransaction`. **Breaking Change**.
 * Add RPC support for Dynamic Fields [[Issue](https://github.com/MystenLabs/sui/pull/5882)].
 * Fold `getRawObject` method into `getObject` method, and use the `DataEncoding` argument to choose between parsedJSON and BCS encoding types.
 * Add new `getCoin` and `getBalance` methods.
 * Standardize the return type of `u64`,`u128`, and `u256` values to `string`.
 * Replace the `get_object_owned_by_object` method with the `get_dynamic_fields` method.
 * Add the `object_type` field to `TransactionEffect` responses.
 * Add the object version and digest field to `Publish` events schema.
 * Remove the `merge_coin`, `split_coin_equal`, and `split_coin` RPC endpoints.
 * Event API: Support using AND/OR operators to combine query criteria.

## SDK (Typescript, Rust)

 * Introduce intent signing support. This includes an `intent` struct to serialize and sign in addition to the transaction data [[Issue](https://github.com/MystenLabs/fastcrypto/issues/26)].
 * Add support to compute transaction digest.

## Sui Move

 * Improve source discoverability.
    * Add the ability for developers to verify source code dependencies against their on-chain counterparts when publishing packages.
    * Enable third-party package managers like Movey to resolve dependencies in Sui Move packages. This enables library developers to distribute their packages under easy-to-identify names. This also removes the error-prone need for developers to remember the GitHub repository, revision, and subdirectories for all their dependencies.
 * Introduce a dev-inspect transaction type that can dry-run any Move function [[RFC](https://github.com/MystenLabs/sui/pull/6538)].
 * Introduce Lamport Versions for Objects. As opposed to the existing mechanism of incrementing an Object's version by one when it gets mutated, all the Objects mutated by a transaction get bumped to the same version, which is the smallest version that's greater than all input versions [[PR](https://github.com/MystenLabs/sui/pull/6163)]. As a result, it is no longer possible to discover previous version(s) of an Object by decrementing its `SequenceNumber` by one, because a transaction could increase an object's version by more than one. We are working on exposing an API to directly obtain an Object's previous version [[Issue](https://github.com/MystenLabs/sui/issues/6529)].
 * Better debug printing for structs (including field names + nice formatting).


## Sui CLI

 * Improve error messaging.
