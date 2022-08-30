---
title: Learning Sui
---

*Sui: pronounced "sweet" without the "T" - with Transactions (loads of them), things are SWEET indeed. :-)*

Welcome to the documentation for the Sui platform. Sui is built on the core [Move](https://github.com/MystenLabs/awesome-move) programming language. This documentation assumes that you have a basic working knowledge of Move. To learn more about the differences between core Move and Sui Move, see [How Sui Move differs from core Move](../learn/sui-move-diffs.md).

For a deep dive into Sui technology, see the [Sui Smart Contracts Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper. Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).

> **Important:** This site is available in two versions in the menu at top left: the default and stable [Devnet](https://docs.sui.io/devnet/learn) branch and the [Latest build](https://docs.sui.io/learn) upstream `main` branch. Use the `devnet` version for app development on top of Sui. Use the Latest build `main` branch for [contributing to the Sui blockchain](../contribute/index.md) itself. Always check and submit fixes to the `main` branch.

## See what's new

### Doc updates

The following list includes the recent updates to Sui and the documentation:

* Follow the [Cryptography (math)](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/math) example for a simple contract that hashes a piece of data using keccak256, recovers a [Secp256k1](https://crates.io/crates/secp256k1/) signature to its public key, and verifies a Secp256k1 signature, producing an event with the results.
* [Bullshark](https://arxiv.org/abs/2201.05677) has replaced Tusk as the consensus component of the [Narwhal](https://github.com/MystenLabs/narwhal)-based [Sui consensus engine](../learn/architecture/consensus.md) for reduced latency and support for fairness with slower validators.
* Sui now supports [shared objects](../build/objects.md#shared) that anyone can read or write to. For an example of creating and accessing a shared object, see [Shared Object](https://examples.sui.io/basics/shared-object.html#shared-object) on https://examples.sui.io/.
* [Sui version 0.7.0](https://github.com/MystenLabs/sui/releases/tag/devnet-0.7.0) is now live in Devnet with numerous fixes and enhancements, including new designs for the [Sui Wallet Browser Extension](../explore/wallet-browser.md) and [Sui Explorer](https://explorer.devnet.sui.io/).
* Interact with the Sui network using our new [Rust SDK](../build/rust-sdk.md), a collection of Rust language [JSON-RPC wrapper and crypto utilities](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk).
* Sui now supports development using [Microsoft Windows 11, macOS, and Linux](../build/install.md#supported-oses). See [install Sui](../build/install.md#prerequisites) for the prerequisites of each operating system.
* This site is now available in two versions in the menu at top left: the default and stable [Devnet](https://docs.sui.io/devnet/learn) branch and the [Latest build](https://docs.sui.io/learn) upstream `main` branch. Use the `devnet` version for app development on top of Sui. Use the Latest build `main` branch for  [contributing to the Sui blockchain](../contribute/index.md) itself. Always check and submit fixes to the `main` branch.

See the Sui `doc/src` [history](https://github.com/MystenLabs/sui/commits/main/doc/src) for a complete changelog of updates to this site. 

### Code changes

Learn about the latest releases in the [#release-notes](https://discord.com/channels/916379725201563759/974444055259910174) channel on Discord.

For a complete view of all changes in the Sui `devnet` branch, see:
https://github.com/MystenLabs/sui/commits/devnet

For upstream updates in the `main` branch, see:
https://github.com/MystenLabs/sui/commits/main

## Kickstart development
The links in the section point to information to help you start working with Sui. 

### Write Smart Contracts with Move
Go to the [Move Quick Start](../build/move/index.md) for information about installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, and invoking your published code.

### Start the Sui network with Sui CLI client
See the [Sui CLI client Quick Start](../build/cli-client.md) for information about installation, querying the chain, client setup, sending transfer transactions, and viewing the effects.

### Take the end-to-end tutorial
Proceed to the [Sui Tutorial](../explore/tutorials.md) for a summary view of setting up your environment, starting a Sui network, gathering accounts and gas, and publishing and playing a game in Sui.

### Program with Objects
Finish with the detailed [Programming with objects](../build/programming-with-objects/index.md) tutorial series offering detailed guidance on manipulating Sui objects, from creation and storage through wrapping and using child objects.

## Navigate this site
Navigate and search this site however you see fit. If you're new to Sui, we recommend that you review the following content in this order:

**Learn** - includes information to help you learn:
* [About Sui](../learn/about-sui.md)
* [How Sui works](../learn/how-sui-works.md)
* [Sui compared to other blockchains](../learn/sui-compared.md)

**Build** - includes information about how to:
* [Install Sui](../build/install.md)
* [Create smart contracts with Move](../build/move/index.md)
* [Set up and configure a local Sui network](../build/cli-client.md)
* [Start a local JSON-RPC Gateway server](../build/json-rpc.md#start-local-rpc-server)

**Explore** - includes more in-depth information about:
* [Sui Wallet](../explore/wallet-browser.md)
* [Devnet](../build/devnet.md)
* [Sui tutorials](../explore/tutorials.md)
* [Sui prototypes](../explore/prototypes.md)
* [Sui examples](../explore/examples.md)  

**Contribute** - includes the following:
* [Frequently Asked Questions](../contribute/faq.md)
* [Logging, Tracing, Metrics, and Observability](../contribute/observability.md)
* [Research Papers](../contribute/research-papers.md)
* [Sui Code of Conduct](../contribute/code-of-conduct.md)
   
**Additional resources** - lets you:
* Employ the [Sui API Reference](https://docs.sui.io/sui-jsonrpc) files for the [Sui JSON-RPC API](../build/json-rpc.md).
* View the [Mysten Labs](https://www.youtube.com/channel/UCI7pCUVxSLcndVhPpZOwZgg) YouTube channel for introductory videos on technology and partners.
