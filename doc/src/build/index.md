---
title: Building Sui
---

Now that you've [learned about Sui](../learn/index.md), it's time to start building.

## Workflow

Here is our recommended workflow to interact with Sui:

1. [Install](../build/install.md) all of the *required tools*.
1. [Quickstart](../build/move.md) Move *smart contract*s:
   1. [Write](../build/move.md#writing-a-package) a package.
   1. [Test](../build/move.md#testing-a-package) a package.
   1. [Debug](../build/move.md#debugging-a-package) a package.
   1. [Publish](../build/move.md#publishing-a-package) a package.
1. [Create](../build/cli-client.md#genesis) and [Start](../build/cli-client.md#starting-the-network) a *local Sui network*.
1. [Start](../build/json-rpc.md#start-local-rpc-server) a *local JSON-RPC Gateway server*.
1. [Connect](../build/cli-client.md#rpc-gateway) to the Sui network Gateway service with the *Sui CLI client*.
1. Build dApps:
   1. [Use](../build/json-rpc.md) *Sui RPC Server and JSON-RPC API* to interact with a local Sui network.
   1. [Employ](../build/sui-json.md) *SuiJSON format* to align JSON inputs more closely with Move call arguments.

Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).
