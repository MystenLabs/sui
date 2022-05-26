---
title: Build a Sui Full Node
---

We welcome you to run your own Sui full node! Sui full nodes run a service that stores the full blockchain state and history. They service reads, either for end clients or by helping other full nodes get up-to-date with the latest transactions that have been committed to the chain.

This role enables [validators](https://docs.sui.io/learn/architecture/validators) (or miners in other networks) to focus on servicing the write path and processing transactions as fast as possible. Once a validator has committed a new set of transactions (or a block of transactions), the validator will push that block to a full node (potentially a number of full nodes) who will then in turn disseminate it to the rest of the network.

**Important**: For potential validators, running a Sui full node is an absolute prerequisite. We encourage auditors, bridges, state mirrors and other interested parties to join us. We offer no guarantees on performance or stability. We’re seeking feedback in the form of [issues filed in GitHub](https://github.com/MystenLabs/sui/issues/new/choose).

## Features

Sui full nodes exist to:

* Track the state of the blockchain, independently and locally.
* Serve read requests from clients.
* Conduct local app testing against verified data.
* [Reward full node runners for their service](../learn/tokenomics.md) with high-quality data.


## State-Synchronization

Today Sui full nodes sync with validators to be able to learn about newly committed transactions. 

The normal life of a transaction requires a few round trips to 2f+1 validators to be able to form a TxCert, at which point a transaction is guaranteed to be committed and executed. 

Today this synchronization process is performed by:

1. Following 2f+1 validators and listening for newly committed transactions.
2. Requesting the transaction from one validator.
3. Locally executing the transaction and updating the local DB.

Today this synchronization process is far from ideal as it requires listening to at a minimum 2f+1 validators to ensure that a full node has properly seen all new transactions. Overtime we will improve this process (e.g. with the introduction of a checkpoints, ability to synchronize with other full nodes, etc) in order to have better guarantees around a full node’s ability to be confident it has seen all recent transactions.

## Architecture

The Sui full node is essentially a read-only view of the network state. Unlike validator nodes, full nodes cannot sign transactions, although they can validate the integrity of the chain by re-executing transactions that were previously committed by a quorum of validators. 

Today a full node is expected to maintain the full history of the chain, although in the future sufficiently old history may need to be pruned and offloaded to cheaper storage. 

Conversely, a validator needs to store only the latest transactions on the *frontier* of the object graph (e.g., txes with >0 unspent output objects).

## Full node setup

Follow the instructions here to run your own Sui full node.

### System requirements


#### Hardware

This is the minimum recommended hardware for running a Sui full node. In Amazon Web Services (AWS), this is known as a t2.large:

* CPUs: 2
* RAM: 8GB
* Disk: 32GB SSD
* Cost/hr: $0.09
* Cost/month: $65

#### Software

Take the normal steps to [install Sui](https://docs.sui.io/build/install) and its prerequisite packages.

Ensure your system can run a Docker image with `docker` or `containerd`.

## Startup

To run a local Sui full node, first [install Sui](https://docs.sui.io/build/install).

Ensure the system has the correct date and time set (recommend UTC).

Then issue these commands in one terminal:

```
$ sui genesis -f
$ sui start & 
```

Followed by this command in a new terminal, run:

```
$ full_node & 
```

This starts a full node at:
http://127.0.0.1:5002

## Use

Now you can use the standard RPC read endpoints to request data. You will do all of this in a third terminal.

To make this easier, set the following environment variable:

```
$ export SUI_RPC_HOST=http://127.0.0.1:5002 
```

Then follow the instructions to employ the [Sui JSON-RPC API](https://docs.sui.io/build/json-rpc#sui-json-rpc-api) using the [SuiJSON format](https://docs.sui.io/build/sui-json).


## Verification

To troubleshoot/test connectivity directly to a validator on TCP port 8080, thereby emulating the full-node-to-validator connection, use the gRPC endpoint that supports the standard grpc health check service to probe the endpoint:
https://github.com/grpc-ecosystem/grpc-health-probe


## Monitoring

Monitor your full node using the instructions at [Logging, Tracing, Metrics, and Observability](https://docs.sui.io/contribute/observability).

## Future plans

In a subsequent release, we may offer automated testing to ensure environments are sufficient to run Sui validator nodes with acceptable performance.

In time, Sui full nodes will operate in a peer-to-peer (p2p) environment where they may poll each other for state. This means that full nodes will have long-established connections between each other and allow for quick dissemination of new transactions/blocks when they are received. This network will enable us to build various p2p applications on top, e.g. state-synchronization and data dissemination.

Sui full nodes don't yet use gossip directly. In future plans, we will likely reuse the follower logic implemented for full node synchronization. Gossip may be employed in the future.

We don't have censorship resistance with the MVP.

We are *not* providing archival nodes containing full history.

Should be similar storage requirements to validator nodes.

it's also about transactions, full nodes don't necessarily require to store txs.

Also we need a single valid copy of data for archiving, we can do a lot of neat tricks on coding theory as well as build incentives such that the actual network acts as a massive archive.
