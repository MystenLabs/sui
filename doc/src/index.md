---
title: Sui Developer Guides
---

Welcome to the documentation for the Sui platform. Since Sui is built upon the core [Move](https://github.com/MystenLabs/awesome-move) programming language,
you should familiarize yourself with it and use this content to apply the differences. For a summary of these differences, see [Why Sui?](learn/sui-move-diffs.md)

For a deep dive into Sui technology, see the [Sui Smart Contracts Platform](paper/sui.pdf) white paper.

## Kickstart development

### Move quick start
See the [Move Quick Start](move.md) for installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, and invoking your published code.
<!--- Then deeper: Sui standard library, design patterns, examples. --->

### Wallet quick start
See the [Wallet Quick Start](wallet.md) for installation, querying the chain, client setup, sending transfer transactions, and viewing the effects.
<!--- Then deeper: wallet CLI vs client service vs forwarder architecture, how to integrate your code (wallet, indexer, ...) with the client service or forwarder components. --->

## Navigate this site

Navigate and search this site however you see fit. Here is the order we recommend if you are new to Sui:

1. [Learn](learn/index.md) about Sui, how it differs from Move, and why you should employ it.
1. [Build](build/index.md) smart contracts, wallets, authorities, transactions, and more.
1. [Explore](explore/index.md) NFTs, make transfers, and see the Sui API.
1. [Contribute]contribute/index.md) to Sui by joining the community, making enhancements, and learning about Mysten Labs.


## Use supporting sites

Take note of these related repositories of information to make best use of the knowledge here:

* [Core Move](https://github.com/diem/move/tree/main/language/documentation) documentation, including:
  * [Tutorial](https://github.com/diem/move/blob/main/language/documentation/tutorial/README.md) - A step-by-step guide through writing a Move module.
  * [Book](https://github.com/diem/move/blob/main/language/documentation/book/src/introduction.md) - A summary with pages on [various topics](https://github.com/diem/move/tree/main/language/documentation/book/src).
  * [Examples](https://github.com/diem/move/tree/main/language/documentation/examples/experimental) - A set of samples, such as for [defining a coin](https://github.com/diem/move/tree/main/language/documentation/examples/experimental/basic-coin) and [swapping it](https://github.com/diem/move/tree/main/language/documentation/examples/experimental/coin-swap).
* [Awesome Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) - A summary of resources related to Move, from blockchains through code samples.
* [Sui API Reference](https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1 ) - The reference files for the Sui Rest API.
