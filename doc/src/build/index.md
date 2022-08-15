---
title: Building Sui
---

Now that you've [learned about Sui](../learn/index.md), it's time to start building.

## Workflow

Here is our recommended workflow to interact with Sui:

1. [Install](../build/install.md) all of the *required tools*.
1. [Connect](../explore/devnet.md) to the Sui Devnet network.
1. [Create](../build/move/index.md) *smart contracts* with Move:
   1. [Write](../build/move/write-package.md) a package.
   1. [Build and test](../build/move/build-test.md) a package.
   1. [Debug and publish](../build/move/debug-publish.md) a package.
1. [Program objects](../build/programming-with-objects/index.md) in Sui.
1. [Start](../build/json-rpc.md) a *JSON-RPC Gateway server* to communicate with Sui.
1. [Talk](../build/comms.md) with Sui using our API and SDKs:
   * [Use](../build/json-rpc.md) the *Sui RPC Server and JSON-RPC API* to interact with a local Sui network.
   * [Employ](../build/sui-json.md) *SuiJSON format* to align JSON inputs more closely with Move call arguments.
   * [Follow](https://docs.sui.io/sui-jsonrpc) the Sui API Reference.
   * [Make](../build/rust-sdk.md) Rust SDK calls to Sui from your app.
   * [Write](https://github.com/MystenLabs/sui/tree/main/sdk/typescript/) TypeScript/JavaScript apps.
   * [Run](../build/fullnode.md) a Sui Fullnode and [subscribe](../build/pubsub.md) to events.
1. Optionally, [create](../build/cli-client.md#genesis) and [start](../build/cli-client.md#starting-the-network) a *local Sui network* to contribute to the blockchain.

Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).
