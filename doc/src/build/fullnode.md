---
title: Run a Sui Full Node
---

**Note:** These instructions are for advanced users. If you just need a local development environment, you should instead follow the instructions in [Create a Local Sui Network](sui-local-network.md) to create a local Full node, validators, and faucet.

Sui Full nodes validate blockchain activities, including transactions, checkpoints, and epoch changes. Each Full node stores and services the queries for the blockchain state and history.

This role enables [validators](../learn/architecture/validators.md) to focus on servicing and processing transactions. When a validator commits a new set of transactions (or a block of transactions), the validator pushes that block to all connected Full nodes that then service the queries from clients.

## Features

Sui Full nodes:

- Track and verify the state of the blockchain, independently and locally.
- Serve read requests from clients.

## State synchronization

Sui Full nodes sync with validators to receive new transactions on the network.

A transaction requires a few round trips to 2f+1 validators to form a transaction certificate (TxCert).

This synchronization process includes:

1.  Following 2f+1 validators and listening for newly committed transactions.
1.  Making sure that 2f+1 validators recognize the transaction and that it reaches finality.
1.  Executing the transaction locally and updating the local DB.

This synchronization process requires listening to at a minimum 2f+1 validators to ensure that a Full node has properly processed all new transactions. Sui will improve the synchronization process with the introduction of checkpoints and the ability to synchronize with other Full nodes.

## Architecture

A Sui Full node is essentially a read-only view of the network state. Unlike validator nodes, Full nodes cannot sign transactions, although they can validate the integrity of the chain by re-executing transactions that a quorum of validators previously committed.

Today, a Sui Full node maintains the full history of the chain.

Validator nodes store only the latest transactions on the _frontier_ of the object graph (for example, transactions with >0 unspent output objects).

## Full node setup

Follow the instructions here to run your own Sui Full.

### Hardware requirements

Suggested minimum hardware to run a Sui Full node:

- CPUs: 8 physical cores / 16 vCPUs
- RAM: 128 GB
- Storage (SSD): 2 TB NVMe drive

### Software requirements

Sui recommends running Sui Full nodes on Linux. Sui supports the Ubuntu and
Debian distributions. You can also run a Sui Full node on macOS.

Make sure to update [Rust](../build/install.md#rust).

Use the following command to install additional Linux dependencies.

```shell
sudo apt-get update \
&& sudo apt-get install -y --no-install-recommends \
tzdata \
libprotobuf-dev \
ca-certificates \
build-essential \
libssl-dev \
libclang-dev \
pkg-config \
openssl \
protobuf-compiler \
git \
clang \
cmake
```

## Configure a Full node

You can configure a Sui Full node either using Docker or by building from
source.

### Using Docker Compose

Follow the instructions in the [Full node Docker Readme](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#readme) to run a Sui Full node using Docker, including [resetting the environment](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#reset-the-environment).

### Setting up a local Sui repository

You must get the latest source files from the Sui GitHub repository.

1. Set up your fork of the Sui repository:
   1. Go to the [Sui repository](https://github.com/MystenLabs/sui) on GitHub
      and click the _Fork_ button in the top right-hand corner of the screen.
   1. Clone your personal fork of the Sui repository to your local machine
      (ensure that you insert your GitHub username into the URL):
      ```shell
      git clone https://github.com/<YOUR-GITHUB-USERNAME>/sui.git
      ```
1. `cd` into your `sui` repository:
   ```shell
   cd sui
   ```
1. Set up the Sui repository as a git remote:
   ```shell
   git remote add upstream https://github.com/MystenLabs/sui
   ```
1. Sync your fork:
   ```shell
   git fetch upstream
   ```
1. Check out the branch associated with the network version you want to run (for example, `devnet` to run a Devnet Full node):
   ```shell
   git checkout --track upstream/<BRANCH-NAME>
   ```

### Setting up a Full node from source

Open a Terminal or Console to the `sui` directory you downloaded in the previous steps to complete the following:

1.  Install the required [Prerequisites](../build/install.md#prerequisites).
1.  Make a copy of the [Full node YAML template](https://github.com/MystenLabs/sui/blob/main/crates/sui-config/data/fullnode-template.yaml):
    ```shell
    cp crates/sui-config/data/fullnode-template.yaml fullnode.yaml
    ```
1.  Download the genesis blob for the network to use:
    - [Devnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob):
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
      ```
    - [Testnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob) - Supported only when there is an active public Testnet network.
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob
      ```
1.  Optional: Skip this step to accept the default paths to resources. Edit the `fullnode.yaml` file to use custom paths.

- Update the `db-path` field with the path to the Full node database.
  ```yaml
  db-path: "/db-files/sui-fullnode"
  ```
- Update the `genesis-file-location` with the path to `genesis.blob`.
  ```yaml
  genesis:
    genesis-file-location: "/sui-fullnode/genesis.blob"
  ```

### Starting services

At this point, your Sui Full node is ready to connect to the Sui network.

1.  Open a Terminal or Console to the `sui` directory.
1.  Start the Sui Full node:
    ```shell
    cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```
1.  Optional: [Publish/subscribe](event_api.md#subscribe-to-sui-events) to notifications using JSON-RPC via websocket.

If your setup is successful, your Sui Full node is now connected to the appropriate network.

Your Full node serves the read endpoints of the [Sui JSON-RPC API](../build/json-rpc.md#sui-json-rpc-api) at: `http://127.0.0.1:9000`.

### Troubleshooting

If you receive a `cannot find -lpq` error, you are missing the `libpq` library. Use `sudo apt-get install libpq-dev` to install on Linux, or `brew install libpq` on MacOS. After you install on MacOS, create a Homebrew link using `brew link --force libpq`. For further context, reference the [issue on Stack Overflow](https://stackoverflow.com/questions/70313347/ld-library-not-found-for-lpq-when-build-rust-in-macos?rq=1).

If you receive the following error:

```
panicked at 'error binding to 0.0.0.0:9184: error creating server listener: Address already in use (os error 98)
```

Then update the metrics address in your fullnode.yaml file to use port `9180`.

```
metrics-address: "0.0.0.0:9180"
```

## Sui Explorer with your Full node

[Sui Explorer](https://explorer.sui.io/) supports connections to custom RPC URLS and local networks. You can point the Explorer to your local Full node and see the
transactions it syncs from the network. To make this change:

1.  Open a browser and go to: https://explorer.sui.io/
1.  Click the **Devnet** button in the top right-hand corner of Sui Explorer (or menu icon on smaller screens) and select **Local** or **Testnet** from the drop-down menu.
1.  Close the **Choose a Network** menu to see the latest transactions. If you chose the **Local** network, Sui Explorer now uses your local Full node to explore the state of the chain.

## Monitoring

Monitor your Full node using the instructions at [Logging, Tracing, Metrics, and Observability](../contribute/observability.md).

Note the default metrics port is `9184`. To change the port, edit your `fullnode.yaml` file.

## Update your Full node

Whenever Sui releases a new version, the network resets and restarts as a new network with no data. You must update your Full node with each Sui release to ensure compatibility with the network.

### Update with Docker Compose

Follow the instructions to [reset the environment](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#reset-the-environment),
namely by running the command:

```shell
docker-compose down --volumes
```

### Update from source

If you followed the instructions for [Building from Source](#building-from-source), update your Full node as follows:

1.  Shut down your running Full node.
1.  `cd` into your local Sui repository:
    ```shell
    cd sui
    ```
1.  Remove the old on-disk database and 'genesis.blob' file:
    ```shell
    rm -r suidb genesis.blob
    ```
1.  Fetch the source from the latest release:
    ```shell
    git fetch upstream
    ```
1.  Reset your branch:
    ```shell
    git checkout -B <BRANCH-NAME> --track upstream/<BRANCH-NAME>
    ```
1.  Download the latest genesis blob:
    - [Devnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob):
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
      ```
    - [Testnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob) - supported only when there is an active public Testnet network
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob
      ```
1.  Update your `fullnode.yaml` configuration file if needed.
1.  Restart your Sui Full node:
    ```shell
    cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```

Your Full node starts on: `http://127.0.0.1:9000`.
