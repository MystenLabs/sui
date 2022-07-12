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

* [Sui Move is feature complete](https://sui.io/resources-move/why-we-created-sui-move/) and ready for you to write safe and efficient smart contracts. See https://examples.sui.io/ to learn Sui Move by example. 
* If your application is written in JavaScript or TypeScript, follow the [TypeScript SDK documentation](https://github.com/MystenLabs/sui/tree/main/sdk/typescript) and [reference files](https://www.npmjs.com/package/@mysten/sui.js).
* Employ the enhanced [Move Visual Studio Code (VSCode) plugin](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer) as described in the [related announcement](https://sui.io/resources-sui/announcing-enhanced-move-vs-code-plugin).
* Get ready to participate in [Sui Incentivized Testnet](https://sui.io/resources-sui/announcing-sui-incentivized-testnet/)!
* The former `wallet` binary has been replaced with the [Sui CLI client](../build/cli-client.md) and combined with related functions.
* [JSON-RPC PubSub](../build/pubsub.md) is supported by Sui [fullnode](../build/fullnode.md) to publish / subscribe using notifications via websocket.
* [Docker Compose](../build/fullnode.md#using-docker-compose) enables simple creation of Sui Fullnodes using [Docker](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#readme).
* [Run a fullnode](../build/fullnode.md) in Sui to have your own local copy of full blockchain state, contribute to Sui, and qualify to be a potential validator.
* Sui [version 0.5.0](https://github.com/MystenLabs/sui/releases/tag/devnet-0.5.0-rc) released to DevNet. See [RELEASES](https://github.com/MystenLabs/sui/blob/main/RELEASES.md) for details on other releases.

For a complete view of all changes in the Sui `devnet` branch, see:
https://github.com/MystenLabs/sui/commits/devnet

For upstream updates in the `main` branch, see:
https://github.com/MystenLabs/sui/commits/main

See the Sui `doc/src` [history](https://github.com/MystenLabs/sui/commits/main/doc/src) for a complete changelog of updates to this site. 

## Kickstart development

### Write Smart Contracts with Move
Go to the [Move Quick Start](../build/move.md) for installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, and invoking your published code.

### Start the Sui network with Sui CLI client
See the [Sui CLI client Quick Start](../build/cli-client.md) for installation, querying the chain, client setup, sending transfer transactions, and viewing the effects.

### Take end-to-end tutorial
Proceed to the [Sui Tutorial](../explore/tutorials.md) for a summary view of setting up your environment, starting a Sui network, gathering accounts and gas, and publishing and playing a game in Sui.

### Program with Objects
Finish with the detailed [Programming with objects](../build/programming-with-objects/index.md) tutorial series offering detailed guidance on manipulating Sui objects, from creation and storage through wrapping and using child objects.

## Navigate this site

Navigate and search this site however you see fit. Here is the order we recommend if you are new to Sui:

1. Learn [about Sui](../learn/about-sui.md), how [Sui Move differs from Core Move](../learn/sui-move-diffs.md), and [how Sui works](../learn/how-sui-works.md) starting in this very section.
1. [Build](../build/index.md) smart contracts, the Sui client, a Sui fullnode, and more.
1. [Explore](../explore/index.md) prototypes and examples.
1. [Contribute](../contribute/index.md) to Sui by joining the community, making enhancements, and learning about Mysten Labs.
1. Employ the [Sui API Reference](https://playground.open-rpc.org/?uiSchema%5BappBar%5D%5Bui:splitView%5D=false&schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/sui/open_rpc/spec/openrpc.json&uiSchema%5BappBar%5D%5Bui:input%5D=false) reference files for the [Sui JSON-RPC API](../build/json-rpc.md).
1. View the [Mysten Labs](https://www.youtube.com/channel/UCI7pCUVxSLcndVhPpZOwZgg) YouTube channel for introductory videos on technology and partners.
