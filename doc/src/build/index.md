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
1. [Create](../build/wallet.md#genesis) and [Start](../build/wallet.md#starting-the-network) a *local Sui network*.
1. [Start](../build/rest-api.md#start-local-rest-server) a *local REST Gateway server*.
1. [Connect](../build/wallet.md#rest-gateway) to the Sui network Gateway service with the *Sui Wallet*.
1. [Take](../build/programming-with-objects/index.md) the *Programming with Objects* codelab.
1. Build dApps:
   1. [Use](../build/rest-api.md) *Sui REST Server and REST API* to interact with a local Sui network.
   1. [Employ](../build/sui-json.md) *SuiJSON format* to align JSON inputs more closely with Move call arguments.


## Related concepts

And if you haven't already, become familiar with these key Sui concepts:

* [Validators](../learn/architecture/validators.md) - The Sui network is operated by a set of independent validators, each running its own instance of the Sui software on a separate machine (or a sharded cluster of machines operated by the same entity).
* [Objects](../build/objects.md) - Sui has programmable objects created and managed by Move packages (a.k.a. smart contracts). Move packages themselves are also objects. Thus, Sui objects can be partitioned into two categories mutable data values and immutable packages.
* [Transactions](../build/transactions.md) - All updates to the Sui ledger happen via a transaction. This section describes the transaction types supported by Sui and explains how their execution changes the ledger.

Find answers to common questions about our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md) and more in our [FAQ](../contribute/faq.md).
