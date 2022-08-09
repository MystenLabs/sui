---
title: Connect and Communicate with the Sui Network
---

Now that you have [installed Sui](install.md), [started the Sui network](cli-client.md), and learned how to [create smark contracts in Move](move/index.md) and [program Sui objects](programming-with-objects/index.md), it's time to let your apps talk to Sui. The pages in this section provide various options for communicating with Sui.

* Use the [Sui CLI client](cli-client.md) to start and set up the Sui network.
* Set up your own [local Sui RPC server and use the Sui JSON-RPC API](json-rpc.md) to interact with a local Sui network.
* Adhere to the [restrictions placed on JSON types](sui-json.md) to make them SuiJSON compatible.
* Interact with the Sui network via the [Sui Rust SDK](rust-sdk.md), a collection of Rust language JSON-RPC wrapper and crypto utilities.
* Sign transactions and interact with the Sui network using the [Sui TypeScript SDK](https://github.com/MystenLabs/sui/tree/main/sdk/typescript) built on the Sui JSON RPC API.
* Run a [Sui Fullnode](fullnode.md) yourself to store the full Sui blockchain state and history and qualify as a [potential validator](https://sui.io/resources-sui/validator-registration-open/).
* Filter and subscribe to a [real-time event stream](pubsub.md) on your Sui Fullnode using JSON-RPC notifications via the WebSocket API.
