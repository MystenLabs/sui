---
title: Learning Sui
---

*Sui: pronounced "sweet" without the "T" - with Transactions (loads of them), things are SWEET indeed. :-)*

Welcome to the documentation for the Sui platform. Since Sui is built upon the core [Move](https://github.com/MystenLabs/awesome-move) programming language, you should familiarize yourself with it and use this content to apply the differences. For a summary of these differences, see [How Sui Move differs from core Move](../learn/sui-move-diffs.md).

For a deep dive into Sui technology, see the [Sui Smart Contracts Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper. Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).

> **Important:** This site is built from the upstream `main` branch and therefore contains updates not yet available on `devnet`.

## See what's new

The following list includes the recent updates to Sui and the documentation:

* Find a list of [single-writer apps](../learn/single-writer-apps.md) that would benefit from Sui's advantages in handling [simple transactions](../learn/how-sui-works.md#simple-transactions).
* Sui [version 0.6.1](https://github.com/MystenLabs/sui/releases/tag/devnet-0.6.1) released to DevNet. See the [#sui-release-notes](https://discord.com/channels/916379725201563759/974444055259910174) channel in Discord for details on this and prior releases.
* Install the [Sui Wallet Browser Extension](../explore/wallet-browser.md) to create NFTs, transfer coins, and carry out common transactions in a Chrome tab.
* [Sui Move is feature complete](https://sui.io/resources-move/why-we-created-sui-move/) and ready for you to write safe and efficient smart contracts. See https://examples.sui.io/ to learn Sui Move by example.
* If your application is written in JavaScript or TypeScript, follow the [TypeScript SDK documentation](https://github.com/MystenLabs/sui/tree/main/sdk/typescript) and [reference files](https://www.npmjs.com/package/@mysten/sui.js).
* Employ the enhanced [Move Visual Studio Code (VSCode) plugin](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer) as described in the [related announcement](https://sui.io/resources-sui/announcing-enhanced-move-vs-code-plugin).
* Get ready to participate in [Sui Incentivized Testnet](https://sui.io/resources-sui/announcing-sui-incentivized-testnet/)!
* The former `wallet` binary has been replaced with the [Sui CLI client](../build/cli-client.md) and combined with related functions.

For a complete view of all changes in the Sui `devnet` branch, see:
https://github.com/MystenLabs/sui/commits/devnet

For upstream updates in the `main` branch, see:
https://github.com/MystenLabs/sui/commits/main

See the Sui `doc/src` [history](https://github.com/MystenLabs/sui/commits/main/doc/src) for a complete changelog of updates to this site. 

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

**Learn** - the Learn section includes information to help you learn:
  * [About Sui](../learn/about-sui.md)
  * [How Sui works](../learn/how-sui-works.md)
  * [Sui compared to other blockchains](../learn/sui-compared.md)

**Build** - the Build section includes information about how to:
  * [Build](../build/index.md) smart contracts, the Sui client, a Sui fullnode, and more

**Explore** - the Explore section includes more in-depth information about Sui:
   * [Explore](../explore/index.md) available environments, tutorials, examples, and prototypes

**Contribute** - the Contribute section includes information about how you can:
   * [Contribute](../contribute/index.md) to Sui by joining the community, making enhancements, and learning about Mysten Labs.

**Additional resources** - the following additional resources contain more information about Sui:
  * Employ the [Sui API Reference](https://playground.open-rpc.org/?schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/crates/sui-open-rpc/spec/openrpc.json) files for the [Sui JSON-RPC API](../build/json-rpc.md).
  * View the [Mysten Labs](https://www.youtube.com/channel/UCI7pCUVxSLcndVhPpZOwZgg) YouTube channel for introductory videos on technology and partners.
  