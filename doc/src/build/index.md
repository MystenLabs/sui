---
title: Building Sui
---

Now that you've [learned about Sui](../learn/index.md), it's time to start building.

## Workflow

Here is our recommended workflow to interact with Sui:

1. [Install](../build/install.md) all of the *required tools*.
1. Interact with the Sui network:
   * Simply [connect](../build/devnet.md) to the Sui Devnet network to begin working with Sui immediately.
   * Optionally, [create](../build/cli-client.md#genesis) and [start](../build/cli-client.md#starting-the-network) a *local Sui network* to contribute to the blockchain.
1. [Create](../build/move/index.md) *smart contracts* with Move:
   1. [Write](../build/move/write-package.md) a package.
   1. [Build and test](../build/move/build-test.md) a package.
   1. [Debug and publish](../build/move/debug-publish.md) a package.
1. [Program objects](../build/programming-with-objects/index.md) in Sui:
   1. [Learn](../build/programming-with-objects/ch1-object-basics.md) object basics.
   1. [Pass](../build/programming-with-objects/ch2-using-objects.md) Move objects as arguments, mutating objects, deleting objects.
   1. [Freeze](../build//programming-with-objects/ch3-immutable-objects.md) an object, using immutable objects.
   1. [Wrap](../build/programming-with-objects/ch4-object-wrapping.md) objects in another object.
   1. [Enable](../build/programming-with-objects/ch5-child-objects.md) objects to own other objects.
1. [Talk](../build/comms.md) with Sui using our API and SDKs:
   * [Use](../build/json-rpc.md) the *Sui RPC Server and JSON-RPC API* to interact with a local Sui network.
   * [Make](../build/rust-sdk.md) Rust SDK calls to Sui from your app.
   * [Make](https://github.com/MystenLabs/sui/tree/main/sdk/typescript/) TypeScript/JavaScript calls to Sui from your apps.
   * [Run](../build/fullnode.md) a Sui Fullnode and [subscribe](../build/pubsub.md) to events.
1. [Reference](../build/reference.md) the format for our API and SuiJSON:
   * [Follow](https://docs.sui.io/sui-jsonrpc) the Sui API Reference.
   * [Employ](../build/sui-json.md) *SuiJSON format* to align JSON inputs more closely with Move call arguments.

Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).
