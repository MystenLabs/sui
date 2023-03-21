---
title: Examples of Sui Smart Contracts
---

Find sample Sui smart contract implementations in the [sui_programmability/examples](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples) directory. Also see [Sui by Example](../explore/move-examples/index.md) for a feature-by-feature guide to [Sui Move](../learn/sui-move-diffs.md).

Here is a list of existing examples in the Sui repository.

## Basics

In the [Basics](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/basics) example, explore object creation, update, and exchange.

## Crypto

In the [Cryptography](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/math) example, employ a simple contract to:
 * Hash a piece of data using keccak256 and output an object with hashed data.
 * Recover a [Secp256k1](https://crates.io/crates/secp256k1/) signature to its public key and output an object with the public key.
 * Verify a Secp256k1 signature and produce an event indicating whether it is verified.

## DeFi

In the [DeFi](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/defi) example, find an atomic swap leveraging an escrow agent that is trusted for liveness, but not safety.

## Fungible Tokens

In the [Fungible Tokens](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/fungible_tokens) example, see a token managed by a treasurer trusted for minting and burning for how (e.g.) a fiat-backed stablecoin would work.

## Games

In the [Games](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/games) example, try out and modify toy games built on top of Sui! These include classic Tic Tac Toe, rock paper scissors, and various versions of an adventure game (Hero).

## NFTs

In the [NFTs](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/nfts) example, browse non-fungible tokens of various types and see NFTs representing assets in a game.
