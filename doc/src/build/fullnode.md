---
title: Run a Sui Fullnode
---

We welcome you to run your own Sui fullnode! Sui fullnodes run a service that
stores the full blockchain state and history. They service reads, either for
end clients or by helping other fullnodes get up-to-date with the latest
transactions that have been committed to the chain.

This role enables
[validators](https://docs.sui.io/learn/architecture/validators) (or miners in
other networks) to focus on servicing the write path and processing
transactions as fast as possible. Once a validator has committed a new set of
transactions (or a block of transactions), the validator will push that block
to a fullnode (potentially a number of fullnodes) who will then in turn
disseminate it to the rest of the network.

**Important**: For potential validators, running a Sui fullnode is an absolute
prerequisite. We encourage auditors, bridges, state mirrors and other
interested parties to join us. At this time we offer no guarantees on performance or
stability of our fullnode software. We expect things to evolve and stabilize
over time and we're seeking feedback in the form of [issues filed in
GitHub](https://github.com/MystenLabs/sui/issues/new/choose) for any issues
encountered.

## Features

Sui fullnodes exist to:

* Track and verify the state of the blockchain, independently and locally.
* Serve read requests from clients.
* Conduct local app testing against verified data.

## State-Synchronization

Today Sui fullnodes sync with validators to be able to learn about newly committed transactions.

The normal life of a transaction requires a few round trips to 2f+1 validators
to be able to form a TxCert, at which point a transaction is guaranteed to be
committed and executed.

Today this synchronization process is performed by:

1. Following 2f+1 validators and listening for newly committed transactions.
2. Requesting the transaction from one validator.
3. Locally executing the transaction and updating the local DB.

Today this synchronization process is far from ideal as it requires listening
to at a minimum 2f+1 validators to ensure that a fullnode has properly seen all
new transactions. Overtime we will improve this process (e.g. with the
introduction of checkpoints, ability to synchronize with other fullnodes,
etc) in order to have better guarantees around a fullnode’s ability to be
confident it has seen all recent transactions.

## Architecture

The Sui fullnode is essentially a read-only view of the network state. Unlike
validator nodes, fullnodes cannot sign transactions, although they can validate
the integrity of the chain by re-executing transactions that were previously
committed by a quorum of validators.

Today, a fullnode is expected to maintain the full history of the chain,
although in the future sufficiently old history may need to be pruned and
offloaded to cheaper storage.

Conversely, a validator needs to store only the latest transactions on the
*frontier* of the object graph (e.g., txes with >0 unspent output objects).

## Fullnode setup

Follow the instructions here to run your own Sui fullnode.

### Hardware requirements

We recommend the following minimum hardware requirements for running a fullnode:

* CPUs: 2
* RAM: 8GB

Storage requirements will vary based on various factors (age of the chain,
transaction rate, etc) although we don't anticipate running a fullnode on
devnet will require more than a handful of GBs given it is reset upon each
release roughly every two weeks.

## Configuring your fullnode

Currently, the only supported way of running a fullnode requires building from
source. In the future, we plan on providing Docker images for more flexibility
in how a fullnode is run.

### Building from source

0. *Prerequisite* Before beginning ensure that the following tools are
   installed in your environment:
    - Rust toolchain managed by [rustup](https://rustup.rs/)
    - `git`
    - `cmake`

1. Set up your fork of the Sui repository:
    - Go to the [Sui repository](https://github.com/MystenLabs/sui) on GitHub
      and click the *Fork* button in the top right-hand corner of the screen.
    - Clone your personal fork of the Sui repository to your local machine
      (ensure that you insert your GitHub username into the URL):
    ```
    $ git clone https://github.com/<YOUR GITHUB USERNAME>/sui.git
    ```
2. `cd` into your `sui` repository:
    ```
    $ cd sui
    ```
3. Set up the Sui repository as a git remote:
    ```
    $ git remote add upstream https://github.com/MystenLabs/sui
    $ git fetch upstream
    ```
4. Check out the 'devnet' branch:
    ```
    $ git checkout --track upstream/devnet
    ```
5. Make a copy of the fullnode configuration template:
   ```
   $ cp crates/sui-config/data/fullnode-template.yaml fullnode.yaml
   ```
6. Download the latest
   [`genesis`](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob)
   state for devnet by clicking the link or by running the following in your
   terminal:
    ```
    $ curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
    ```
7. Edit your `fullnode.yaml` file:
    - Update the `db-path` field with a path to where the fullnode's database
      will be located. By default this will create the database in a directory
      `./suidb` relative to your current directory:
    ```yaml
    db-path: "/path/to/db"
    ```
    - Update the `genesis-file-location` to the path where the `genesis` file
      is located. By default the config looks for a file `genesis.blob` in your
      current directory:
    ```yaml
    genesis:
      genesis-file-location: "/path/to/genesis.blob"
    ```
8. Start your Sui fullnode:
    ```
    $ cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```

Your fullnode will now be serving the read endpoints of the [Sui JSON-RPC
API](../build/json-rpc.md#sui-json-rpc-api) at
`http://127.0.0.1:9000`.

## Using the Explorer with your fullnode

The [Sui Explorer](https://explorer.devnet.sui.io/) supports configuring where
it should issue read requests to query the blockchain. This enables you to
point the explorer at your locally running fullnode and explore the
transactions that it has synced from the network. You can do this by:

1. Open a browser and go to: https://explorer.devnet.sui.io/
2. Click the button in the top right-hand corner of the page and select
   `Local` from the drop-down menu.

The Explorer will now be using your local fullnode to explore the state of the chain.

## Monitoring

Monitor your fullnode using the instructions at [Logging, Tracing, Metrics, and
Observability](https://docs.sui.io/contribute/observability).

## Updating your fullnode with new releases

Whenever a new release is deployed to `devnet`, the blockchain state is
generally wiped clean. In order to have your fullnode continue to properly
synchronize with the new state of devnet, you'll need to follow a few steps
based on how you originally set up your node. See below.

### Built from source

If you followed the [Building from
Source](#markdown-header-building-from-source) directions, update as follows:

1. Shut down your currently running fullnode.
2. `cd` into your local Sui repository:
    ```
    $ cd sui
    ```
3. Remove the old on-disk database and 'genesis.blob' file:
    ```
    $ rm -r suidb genesis.blob
    ```
4. Fetch the source from the latest release:
    ```
    $ git fetch upstream
    $ git checkout -B devnet --track upstream/devnet
    ```
5. Download the latest
   [`genesis`](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob)
   state for devnet.
6. Update your `fullnode.yaml` configuration file if needed.
7. Start your Sui fullnode:
    ```
    $ cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```

## Future plans

Today, a fullnode relies only on synchronizing with 2f+1 validators in order to
ensure that it has seen all committed transactions. In the future, we expect
fullnodes to fully participate in a peer-to-peer (p2p) environment where the
load of disseminating new transactions can be shared with the whole network and
not have the burden be solely on the validators. We also expect future
features, such as checkpoints, to enable improved performance of synchronizing the
state of the chain from genesis.

Please see our privacy policy to learn how we handle information about our nodes.
