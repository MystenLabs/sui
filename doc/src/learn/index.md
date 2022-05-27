---
title: Learning Sui
---

*Sui: pronounced "sweet" without the "T" - with Transactions (loads of them), things are SWEET indeed. :-)*

Welcome to the documentation for the Sui platform. Since Sui is built upon the core [Move](https://github.com/MystenLabs/awesome-move)
programming language, you should familiarize yourself with it and use this content to apply the differences. For a summary of these differences, see
[Sui compared to other blockchains](../learn/sui-compared.md).

For a deep dive into Sui technology, see the [Sui Smart Contracts Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper. Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).

> **Important:** This site is built from the upstream `main` branch and therefore will contain updates not yet found in `devnet`.

## See what's new

Find the latest updates to these contents in this section:

* [Programming with objects](../build/programming-with-objects/index.md) tutorial series offers detailed guidance on manipulating Sui objects, from creation and storage through wrapping and using child objects.
* Sui [version 0.3.0](https://github.com/MystenLabs/sui/releases/tag/devnet-0.3.0-rc) released to DevNet. See [RELEASES](https://github.com/MystenLabs/sui/blob/main/RELEASES.md) for details on other releases.
* [Sui tokenomics](../learn/tokenomics/index.md) are now fully explained and cover:
  * [Sui token](../learn/tokenomics/sui-token.md).
  * [Gas-pricing mechanism](../learn/tokenomics/gas-pricing.md).
  * [Sui storage fund](../learn/tokenomics/storage-fund.md).
  * [Delegated proof-of-stake system](../learn/tokenomics/proof-of-stake.md).

For a complete view of all changes in the Sui `devnet` branch, see:
https://github.com/MystenLabs/sui/commits/devnet

For upstream updates in the `main` branch, see:
https://github.com/MystenLabs/sui/commits/main

See the Sui `doc/src` [history](https://github.com/MystenLabs/sui/commits/main/doc/src) for a complete changelog of updates to this site. 

## Kickstart development

### Move quick start
Go to the [Move Quick Start](../build/move.md) for installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, and invoking your published code.

### Wallet quick start
See the [Wallet Quick Start](../build/wallet.md) for installation, querying the chain, client setup, sending transfer transactions, and viewing the effects.

### End-to-end tutorial
Finish with the [Sui Tutorial](../explore/tutorials.md) for a summary view of setting up your environment, starting a Sui network, gathering accounts and gas, and publishing and playing a game in Sui.

## Navigate this site

Navigate and search this site however you see fit. Here is the order we recommend if you are new to Sui:

1. Learn [about Sui](../learn/about-sui.md), how [Sui differs from Move](../learn/why-move.md), and [how Sui works](../learn/how-sui-works.md) starting in this very section.
1. [Build](../build/index.md) smart contracts, wallets, validators, transactions, and more.
1. [Explore](../explore/index.md) prototypes and examples.
1. [Contribute](../contribute/index.md) to Sui by joining the community, making enhancements, and learning about Mysten Labs.

## Use supporting sites

Take note of these related repositories of information to make best use of the knowledge here:

* [Move & Sui podcast](https://zeroknowledge.fm/228-2/) on Zero Knowledge where programmable objects are described in detail.
* Original [Move Book](https://move-book.com/index.html) written by a member of the Sui team.
* [Core Move](https://github.com/move-language/move/tree/main/language/documentation) documentation, including:
  * [Tutorial](https://github.com/move-language/move/blob/main/language/documentation/tutorial/README.md) - A step-by-step guide through writing a Move module.
  * [Book](https://github.com/move-language/move/blob/main/language/documentation/book/src/introduction.md) - A summary with pages on [various topics](https://github.com/move-language/move/tree/main/language/documentation/book/src).
  * [Examples](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental) - A set of samples, such as for [defining a coin](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/basic-coin) and [swapping it](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/coin-swap).
* [Awesome Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) - A summary of resources related to Move, from blockchains through code samples.
* [Sui API Reference](https://playground.open-rpc.org/?uiSchema%5BappBar%5D%5Bui:splitView%5D=false&schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/sui/open_rpc/spec/openrpc.json&uiSchema%5BappBar%5D%5Bui:input%5D=false) - The reference files for the [Sui JSON-RPC API](../build/json-rpc.md).
