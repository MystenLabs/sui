---
title: Building Sui
---

Now that you've [learned about Sui](../learn/index.md), it's time to [install](../build/install.md) all the required tools and start building. Then become familiar with these key concepts:

* [Smart Contracts with Move](../build/move.md) - Move is an open source language for writing safe smart contracts. In Sui, Move is used to define,
  create and manage programmable Sui objects representing user-level assets.
* [Sui Wallet](../build/wallet.md) - Sui wallet was developed to facilitate local experimentation with Sui features. In this document, we describe
  how to set up Sui wallet and execute wallet commands through its command line interface, Wallet CLI.
* [Authorities](../build/authorities.md) - The Sui network is operated by a set of independent authorities, each running its own instance of the Sui
  software on a separate machine (or a sharded cluster of machines operated by the same entity).
* [SuiJSON](../build/sui-json.md) - SuiJSON is a JSON-based format with restrictions that allow Sui to align JSON inputs more closely with Move Call
  arguments. This table shows the restrictions placed on JSON types to make them SuiJSON compatible.
* [Objects](../build/objects.md) - Sui has programmable objects created and managed by Move packages (a.k.a. smart contracts). Move packages themselves
  are also objects. Thus, Sui objects can be partitioned into two categories mutable data values and immutable packages.
* [Transactions](../build/transactions.md) - All updates to the Sui ledger happen via a transaction. This section describes the transaction types
  supported by Sui and explains how their execution changes the ledger.
