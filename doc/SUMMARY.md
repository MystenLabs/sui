# Sui Developer Portal

Welcome to Sui, a next generation smart contract platform with high throughput, low latency, and an asset-oriented programming model powered by the [Move](https://github.com/diem/move) programming language! Here are some suggested starting points:

* To jump right into building smart contract applications on top of Sui, go to [Move Quick Start](move.md).
* To experiment with a sample Sui wallet, check out [Wallet Quick Start](wallet.md).
* To understand what's possible by browsing examples of full-fledged applications and games built on top of Sui, review the [Demos](TODO).
* To go deep on how Sui works, understand [Key Concepts](TODO).
* To learn what distinguishes Sui from other blockchain systems, see [What Makes Sui Different?](TODO).
* To experience Sui's speed and scalability for yourself, try [Benchmarking](TODO).
* To see the current status of the Sui software/network and preview what's coming next, read through our [Roadmap](TODO).

TODO: Populate and link to the missing pages above or strike the links and references.

## Architecture

Sui is a distributed ledger that stores a collection of programmable *[objects](objects.md)*, each with a globally unique ID. Every object is owned by a single *address*, and each address can own an arbitrary number of objects.

The ledger is updated via a *[transaction](transactions.md)* sent by a particular address. A transaction can create, destroy, and write objects, as well as transfer them to other addresses.

Structurally, a transaction contains a set of input object references and a pointer to a Move code object that already exists in the ledger. Executing a transaction produces updates to the input objects and (if applicable) a set of freshly created objects along with their owners.

A transaction whose sender is address *A* can accept objects owned only by *A* with one exception: a transaction can read from immutable objects regardless of their owner.

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

Sui authorities process transactions in parallel with high throughput using [Byzantine Consistent Broadcast](https://en.wikipedia.org/wiki/Byzantine_fault).

## Move quick start
See the [Move Quick Start](move.md) for installation, defining custom objects, object operations (create/destroy/update/transfer/freeze), publishing, invoking your published code. Then deeper: Sui standard library, design patterns, examples.

## Wallet quick start
See the [Wallet Quick Start](wallet.md) for installation, querying the chain, client setup, getting test coins, sending transfer transactions, viewing the effects in a block explorer. Then deeper: wallet CLI vs client service vs forwarder architecture, how to integrate your code (wallet, indexer, ...) with the client service or forwarder components.

## Demos

TODO: Develop or remove.

## Concepts
- [Authorities](authorities.md)
- [Objects](objects.md)
- [Programmability](programmability.md)
- [Transactions](transactions.md)
- TODO: clients
- TODO: state sync
- TODO: reconfiguration
- TODO governance, incentives, and rewards

## What makes Sui different?

Sui allows a set of distributed authorities, some of which are Byzantine, to maintain a high-integrity and high-availability settlement system for pre-funded payments. It can be used to settle payments in a native unit of value (crypto-currency) or as a financial side-infrastructure to support retail payments in fiat currencies. Sui is based on Byzantine Consistent Broadcast as its core primitive, foregoing the expenses of full atomic commit channels (consensus). The resulting system has low latency for both confirmation and payment finality. Remarkably, each authority can be sharded across many machines to allow unbounded horizontal scalability. Our experiments demonstrate intra-continental confirmation latency of less than 100ms, making Sui applicable to point-of-sale payments. In laboratory environments, we achieve more than 80,000 transactions per second with 20 authorities---surpassing the requirements of current retail card payment networks while significantly increasing their robustness.

Here are Sui's key features:

- High throughput and low latency (enables low cost with fixed hardware)
- Causal order vs total order (enables massively parallel execution)
- Move and object-centric data model (enables composable objects/NFTs)

### Authorities vs Validators/Miners
An authority plays a role similar to "validators" or "miners" in other blockchain systems. The key distinction between these roles (and the reason we insist on using a separate term) is that validators/miners are *active*, whereas authorities are *passive*. Broadly speaking:
* Miners/validators continuously participate in a global consensus protocol that requires multiple rounds of all-to-all communication between the participants. The goal is typically to agree on a *totally ordered* block of transactions and the result of their execution.
* Authorities do nothing until they receive a transaction or certificate from a user. Upon receiving a transaction or certificate, an authority need not communicate with other authorities in order to take action and advance its internal state machine. It may wish to communicate with other authorities to share certificates but need not do so.

## Causal order vs Total order
Unlike most existing blockchain systems (and as the reader may have guessed from the description of write requests above), Sui does not impose a total order on the transactions submitted by clients. Instead, transactions are *causally* ordered--if a transaction `T1` produces output objects `O1` that are used as input objects in a transaction `T2`, an authority must execute `T1` before it executes `T2`. Note that `T2` need not use these objects directly for a causal relationship to exist--e.g., `T1` might produce output objects which are then used by `T3`, and `T2` might use `T3`'s output objects. However, transactions with no causal relationship can be processed by Sui authorities in any order.

## Benchmarking
TODO: installation, running throughput and latency benchmarks, demonstrating that throughput goes up when you add more cores, adding more authorities leads to linear-ish decrease of latency/throughput.

## Roadmap

TODO
