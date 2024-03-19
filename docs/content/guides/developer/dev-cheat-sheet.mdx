---
title: Sui Developer Cheat Sheet
sidebar_label: Dev Cheat Sheet
---

Quick reference on best practices for Sui Network developers.

## Move

### General

- Read about package upgrades and write upgrade-friendly code:
  - Packages are immutable, so buggy package code can be called forever. Add protections at the object level instead.
  - If you upgrade a package `P` to `P'`, other packages and clients that depend on `P` will continue using `P`, not auto-update to `P'`. Both dependent packages and client code must be explicitly updated to point at `P'`.
  - Packages that expect to be extended by dependent packages can avoid breaking their extensions with each upgrade by providing a standard (unchanging) interface that all versions conform to. See this example for [message sending](https://github.com/wormhole-foundation/wormhole/blob/74dea3bf22f0e27628b432c3e9eac05c85786a99/sui/wormhole/sources/publish_message.move) across a bridge from Wormhole. Extension packages that produce messages to send can use [`prepare_message`](https://github.com/wormhole-foundation/wormhole/blob/74dea3bf22f0e27628b432c3e9eac05c85786a99/sui/wormhole/sources/publish_message.move#L68-L90) from any version of the Wormhole package to produce a [`MessageTicket`](https://github.com/wormhole-foundation/wormhole/blob/74dea3bf22f0e27628b432c3e9eac05c85786a99/sui/wormhole/sources/publish_message.move#L52-L66) while client code to send the message must pass that `MessageTicket` into [`publish_message`](https://github.com/wormhole-foundation/wormhole/blob/74dea3bf22f0e27628b432c3e9eac05c85786a99/sui/wormhole/sources/publish_message.move#L92-L152) in the latest version of the package.
  - `public` function signatures cannot be deleted or changed, but `public(friend)` functions can. Use `public(friend)` or private visibility liberally unless you are exposing library functions that will live forever.
  - It is not possible to delete `struct` types, add new fields (though you can add dynamic fields), or add new [abilities](https://move-language.github.io/move/abilities.html) via an upgrade. Introduce new types carefully—they will live forever!
- Use `vector`-backed collections (`vector`, `VecSet`, `VecMap`, `PriorityQueue`) with a **known** maximum size of ≤ 1000 items.
  - Use dynamic field-backed collections (`Table`, `Bag`, `ObjectBag`, `ObjectTable`, `LinkedTable`) for any collection that allows third-party addition, larger collections, and collections of unknown size.
  - Move objects have a maximum size of 250KB—any attempt to create a larger object leads to an aborted transaction. Ensure that your objects do not have an ever-growing `vector`-backed collection.
- If your function `f` needs a payment in (e.g.) SUI from the caller, use `fun f(payment: Coin<SUI>)` not `fun f(payment: &mut Coin<SUI>, amount: u64)`. This is safer for callers—they know exactly how much they are paying, and do not need to trust `f` to extract the right amount.
- Don't micro-optimize gas usage. Sui computation costs are rounded up to the closest _bucket_, so only very drastic changes will make a difference. In particular, if your transaction is already in the lowest cost bucket, it can't get any cheaper.
- Follow the [Move coding conventions](https://move-language.github.io/move/coding-conventions.html) for consistent style.

### Composability

- Use the `display` standard to customize how your objects show up in wallets, apps, and explorers
- Avoid “self-transfers”—whenever possible, instead of writing `transfer::transfer(obj, tx_context::sender(ctx))`, return `obj` from the current function. This allows a caller or programmable transaction block to use `obj`.

### Testing

- Use [`sui::test_scenario`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/packages/sui-framework/sources/test/test_scenario.move) to mimic multi-transaction, multi-sender test scenarios.
- Use the [`sui::test_utils`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/packages/sui-framework/sources/test/test_utils.move#L5) module for better test error messages via `assert_eq`, debug printing via `print`, and test-only destruction via `destroy`.
- Use `sui move test --coverage` to compute code coverage information for your tests, and `sui move coverage source --module <name>` to see uncovered lines highlighted in red. Push coverage all the way to 100% if feasible.

## Apps

- For optimal performance and data consistency, apps should submit writes and reads for the same full node. In the TS SDK, this means that apps should use the wallet's [`signTransactionBlock`](https://sdk.mystenlabs.com/dapp-kit) API, then submit the transaction via a call to [`execute_transactionBlock`](/sui-api-ref#sui_executetransactionblock) on the app's full node, _not_ use the wallet's `signAndExecuteTransactionBlock` API. This ensures read-after-write-consistency--reads from the app's full node will reflect writes from the transaction right away instead of waiting for a checkpoint.
- For lower latency, use [`executeTransactionBlock`](/sui-api-ref#sui_executetransactionblock) with `"showEffects": false` and `"showEvents": false` if your app needs to know that a transaction was confirmed, but does not immediately need to see the transaction effects or read the objects/events written by the transaction.
- Apps should implement a local cache for frequently read data rather than over-fetching from the full node.
- Whenever possible, use programmable transaction blocks to compose existing on-chain functionality rather than publishing new smart contract code. Programmable transaction blocks allow large-scale batching and heterogeneous composition, driving already-low gas fees down even further.
- Apps should leave gas budget, gas price, and coin selection to the wallet. This gives wallets more flexibility, and it's the wallet's responsibility to dry run a transaction to ensure it doesn't fail.

## Signing

- **Never** sign two concurrent transactions that are touching the same owned object. Either use independent owned objects, or wait for one transaction to conclude before sending the next one. Violating this rule might lead to client [equivocation](/references/sui-glossary.mdx#equivocation), which locks up the owned objects involved in the two transactions until the end of the current epoch.
- Any `sui client` command that crafts a transaction (e.g., `sui client publish`, `sui client call`) can accept the `--serialize-output` flag to output a base64 transaction to be signed.
- Sui supports several signature schemes for transaction signing, including native multisig.

## zkLogin

- Call the proving service as sparingly as possible. Design your app flows such that you call the proving service only when the user is about to perform a real transaction.
- Beware of how you cache the ephemeral private key. Treat the private key akin to a piece of highly sensitive data, e.g., password. If an (unexpired) ephemeral private key and its corresponding ZK proof are leaked, then an attacker can steal user's assets.
