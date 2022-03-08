# Sui Developer Portal

Welcome to Sui, a next generation smart contract platform with high throughput, low latency, and an asset-oriented programming model powered by the [Move](https://github.com/MystenLabs/awesome-move) programming language! Here are some suggested starting points:

* To jump right into building smart contract applications on top of Sui, go to [Move Quick Start](https://github.com/MystenLabs/sui/tree/main/doc/src/move.md).
* To experiment with a sample Sui wallet, check out [Wallet Quick Start](https://github.com/MystenLabs/sui/tree/main/doc/src/wallet.md).
* To go deep on how Sui works, read the Sui [white paper](https://github.com/MystenLabs/sui/tree/main/doc/paper/sui.pdf) and check out [Key Concepts](https://github.com/MystenLabs/sui/tree/main/doc/src/key-concepts.md).
<!---* To understand what's possible by browsing examples of full-fledged applications and games built on top of Sui, review the [Demos](TODO).--->
* To understand what's possible by browsing Move code built on top of Sui, review the [examples](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/sources)
* To start coding against Sui's REST API's, start [here](https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1)
* To learn what distinguishes Sui from other blockchain systems, see [What Makes Sui Different?](https://github.com/MystenLabs/sui/tree/main/doc/src/what-makes-sui-different.md).
<!---* To experience Sui's speed and scalability for yourself, try [Benchmarking](TODO).--->
<!---* To see the current status of the Sui software/network and preview what's coming next, read through our [Roadmap](TODO).--->

<!---TODO: Populate and link to the missing pages above or strike the links and references.--->

## Architecture

Sui is a distributed ledger that stores a collection of programmable *[objects](https://github.com/MystenLabs/sui/tree/main/doc/src/objects.md)*, each with a globally unique ID. Every object is owned by a single *address*, and each address can own an arbitrary number of objects.

The ledger is updated via a *[transaction](https://github.com/MystenLabs/sui/tree/main/doc/src/transactions.md)* sent by a particular address. A transaction can create, destroy, and write objects, as well as transfer them to other addresses.

Structurally, a transaction contains a set of input object references and a pointer to a Move code object that already exists in the ledger. Executing a transaction produces updates to the input objects and (if applicable) a set of freshly created objects along with their owners. A transaction whose sender is address *A* can accept objects owned by *A*, shared objects, and objects owned by other objects in the first two groups as input.

```mermaid
flowchart LR
    CC(CLI Client) --> ClientService
    RC(Rest Client) --> ClientService
    RPCC(RPC Client) --> ClientService
    ClientService --> AuthorityAggregator
    AuthorityAggregator --> AC1[AuthorityClient] & AC2[AuthorityClient]
    subgraph Authority1
      AS[AuthorityState]
    end
    subgraph Authority2
      AS2[AuthorityState]
    end
    AC1 <==>|Network TCP| Authority1
    AC2 <==>|Network TCP| Authority2
```

Sui authorities agree on and execute transactions in parallel with high throughput using [Byzantine Consistent Broadcast](https://en.wikipedia.org/wiki/Byzantine_fault).

## Move quick start
See the [Move Quick Start](https://github.com/MystenLabs/sui/tree/main/doc/src/move.md) for installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, and invoking your published code.
<!--- Then deeper: Sui standard library, design patterns, examples. --->

## Wallet quick start
See the [Wallet Quick Start](https://github.com/MystenLabs/sui/tree/main/doc/src/wallet.md) for installation, querying the chain, client setup, sending transfer transactions, and viewing the effects.
<!--- Then deeper: wallet CLI vs client service vs forwarder architecture, how to integrate your code (wallet, indexer, ...) with the client service or forwarder components. --->
