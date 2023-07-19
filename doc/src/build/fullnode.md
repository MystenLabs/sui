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
- Storage (SSD): 4 TB NVMe drive

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
   1. Go to the [Sui repository](https://github.com/MystenLabs/sui) on GitHub and click the **Fork** button in the top right-hand corner of the screen.
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

1. Install the required [Prerequisites](../build/install.md#prerequisites).
1. Make a copy of the [Full node YAML template](https://github.com/MystenLabs/sui/blob/main/crates/sui-config/data/fullnode-template.yaml):
    ```shell
    cp crates/sui-config/data/fullnode-template.yaml fullnode.yaml
    ```
1. Download the genesis blob for the network to use:
    - [Devnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob):
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
      ```
    - [Testnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob):
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/testnet/genesis.blob
      ```
    - [Mainnet genesis blob](https://github.com/MystenLabs/sui-genesis/raw/main/mainnet/genesis.blob)
      ```shell
      curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/mainnet/genesis.blob
      ```
1. Testnet and Mainnet Full nodes only: Edit the `fullnode.yaml` file to include peer nodes for state synchronization. Append the following to the end of the current configuration:
     * **Testnet**
       ```shell
       p2p-config:
         seed-peers:
           - address: /dns/ewr-tnt-ssfn-00.testnet.sui.io/udp/8084
             peer-id: df8a8d128051c249e224f95fcc463f518a0ebed8986bbdcc11ed751181fecd38
           - address: /dns/lax-tnt-ssfn-00.testnet.sui.io/udp/8084
             peer-id: f9a72a0a6c17eed09c27898eab389add704777c03e135846da2428f516a0c11d
           - address: /dns/lhr-tnt-ssfn-00.testnet.sui.io/udp/8084
             peer-id: 9393d6056bb9c9d8475a3cf3525c747257f17c6a698a7062cbbd1875bc6ef71e
           - address: /dns/mel-tnt-ssfn-00.testnet.sui.io/udp/8084
             peer-id: c88742f46e66a11cb8c84aca488065661401ef66f726cb9afeb8a5786d83456e
        ```
     * **Mainnet**
       ```shell
       p2p-config:
         seed-peers:
           - address: /dns/icn-00.mainnet.sui.io/udp/8084
             peer-id: 303f1f35afc9a6f82f8d21724f44e1245f4d8eca0806713a07c525dadda95a66
           - address: /dns/icn-01.mainnet.sui.io/udp/8084
             peer-id: cb7ce193cf7a41e9cc2f99e65dd1487b6314a57c74be42cc8c9225b203301812
           - address: /dns/mel-00.mainnet.sui.io/udp/8084
             peer-id: d32b55bdf1737ec415df8c88b3bf91e194b59ee3127e3f38ea46fd88ba2e7849
           - address: /dns/mel-01.mainnet.sui.io/udp/8084
             peer-id: bbf3be337fc16614a1953da83db729abfdc40596e197f36fe408574f7c9b780e
           - address: /dns/ewr-00.mainnet.sui.io/udp/8084
             peer-id: c7bf6cb93ca8fdda655c47ebb85ace28e6931464564332bf63e27e90199c50ee
           - address: /dns/ewr-01.mainnet.sui.io/udp/8084
             peer-id: 3227f8a05f0faa1a197c075d31135a366a1c6f3d4872cb8af66c14dea3e0eb66
           - address: /dns/sjc-00.mainnet.sui.io/udp/8084
             peer-id: 6f0b25087cd6b2fd2e4329bcf308ac95a37c49277dd7286b72470c124809db5b
           - address: /dns/sjc-01.mainnet.sui.io/udp/8084
             peer-id: af1d5d8468b3612ac2b6ff3ca91e99a71390dbe5b83dea9f6ae2da708d689227
           - address: /dns/lhr-00.mainnet.sui.io/udp/8084
             peer-id: c619a5e0f8f36eac45118c1f8bda28f0f508e2839042781f1d4a9818043f732c
           - address: /dns/lhr-01.mainnet.sui.io/udp/8084
             peer-id: 53dcedf250f73b1ec83250614498947db00d17c0181020fcdb7b6db12afbc175
1. Optional: Skip this step to accept the default paths to resources. Edit the `fullnode.yaml` file to use custom paths.
   - Update the `db-path` field with the path to the Full node database.
     ```yaml
     db-path: "/db-files/sui-fullnode"
     ```
   - Update the `genesis-file-location` with the path to `genesis.blob`.
     ```yaml
     genesis:
       genesis-file-location: "/sui-fullnode/genesis.blob"
     ```
1. Optional: To save disk space on your Full node, add the following settings to your `fullnode.yaml` file to enable aggressive pruning:
   ```
   authority-store-pruning-config:
     num-latest-epoch-dbs-to-retain: 3
     epoch-db-pruning-period-secs: 3600
     num-epochs-to-retain: 0
     max-checkpoints-in-batch: 10
     max-transactions-in-batch: 1000
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

[Sui Explorer](https://suiexplorer.com/) supports connections to custom RPC URLS and local networks. You can point the Explorer to your local Full node and see the transactions it syncs from the network.

1.  Open a browser and go to: https://suiexplorer.com/
1.  Click **Mainnet** in the network drop-down at the top right-hand corner (or three bars on smaller screens) and select **Local** to connect to a local network, or select **Custom RPC URL** and then enter the URL.

Sui Explorer displays information about the selected network.

## Monitoring

Monitor your Full node using the instructions at [Logging, Tracing, Metrics, and Observability](../contribute/observability.md).

The default metrics port is `9184`. To change the port, edit your `fullnode.yaml` file.

## Update your Full node

Whenever Sui releases a new version, you must update your Full node with the release to ensure compatibility with the network it connects to. For example, if you use Sui Testnet you should install the version of Sui running on Sui Testnet. 

### Update with Docker Compose

Follow the instructions to [reset the environment](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#reset-the-environment),
namely by running the command:

```shell
docker-compose down --volumes
```

### Update from source

If you followed the instructions for [Building from Source](#building-from-source), use the following steps to update your Full node:

1.  Shut down your running Full node.
1.  `cd` into your local Sui repository:
    ```shell
    cd sui
    ```
1.  Remove the database and 'genesis.blob' file:
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

## Object pruning

Sui adds new object versions to the database as part of transaction execution. This makes previous versions ready for 
garbage collection. However, without pruning, this can result in database performance degradation and requires large 
amounts of storage space. Sui identifies the objects that are eligible for pruning in each checkpoint, and then performs
the pruning in the background.

You can enable pruning for a Sui node by adding the `authority-store-pruning-config` config to `fullnode.yaml` file:
```yaml
authority-store-pruning-config:
  # Number of epoch dbs to keep 
  # Not relevant for object pruning
  num-latest-epoch-dbs-to-retain: 3
  # The amount of time, in seconds, between running the object pruning task.
  # Not relevant for object pruning
  epoch-db-pruning-period-secs: 3600
  # Number of epochs to wait before performing object pruning.
  # When set to 0, Sui prunes old object versions as soon
  # as possible. This is also called *aggressive pruning*, and results in the most effective
  # garbage collection method with the lowest disk usage possible. 
  # This is the recommended setting for Sui Validator nodes since older object versions aren't
  # necessary to execute transactions.
  # When set to1, Sui prunes only object versions from transaction checkpoints
  # previous to the current epoch. In general, when set to N (where N >= 1), Sui prunes  
  # only object versions from checkpoints up to `current - N` epoch. 
  # It is therefore possible to have multiple versions of an object present 
  # in the database. This setting is recommended for Sui Full nodes as they might need to serve 
  # RPC requests that require looking up objects by ID and Version (rather than just latest
  # version). However, if your Full node does not serve RPC requests you should then also enable  
  # aggressive pruning.
  num-epochs-to-retain: 0
  # Advanced setting: Maximum number of checkpoints to prune in a batch. The default
  # settings are appropriate for most use cases.
  max-checkpoints-in-batch: 10
  # Advanced setting: Maximum number of transactions in one batch of pruning run. The default
  # settings are appropriate for most use cases.
  max-transactions-in-batch: 1000
```
## Transaction pruning

Transaction pruning removes previous transactions and effects from the database.
Sui periodically creates checkpoints. Each checkpoint contains the transactions that occurred during the checkpoint and their associated effects.
Sui performs transaction pruning in the background after checkpoints complete.
You can enable transaction pruning for your Full node or Validator node by adding  `num_epochs_to_retain_for_checkpoints`
to the `authority-store-pruning-config` config for the node:

```yaml
authority-store-pruning-config:
  num-latest-epoch-dbs-to-retain: 3
  epoch-db-pruning-period-secs: 3600
  num-epochs-to-retain: 0
  max-checkpoints-in-batch: 10
  max-transactions-in-batch: 1000
  # Number of epochs to wait before performing transaction pruning.
  # When this is N (where N >= 2), Sui prunes transactions and effects from 
  # checkpoints up to the `current - N` epoch. Sui never prunes transactions and effects from the current and
  # immediately prior epoch. N = 2 is a recommended setting for Sui Validator nodes and Sui Full nodes that don't 
  # serve RPC requests.
  num_epochs_to_retain_for_checkpoints: 2
  # Ensures that individual database files periodically go through the compaction process.
  # This helps reclaim disk space and avoid fragmentation issues
  periodic-compaction-threshold-days: 1
```

## Archival Fallback

After Sui starts performing transaction pruning on Full nodes to remove historical transactions and their effects, 
it might not be possible for peer nodes to catch up with the transactions and effects via synchronization. Instead, peer
nodes can fall back by downloading this data from an archive.
The archive is a history of all transaction data on Sui, trailing behind the latest checkpoint by 10 minutes. 
You should enable this for all nodes as a best practice. To configure a Sui node to automatically fall back to a 
snapshot image, add the following to your `fullnode.yaml` file.:

```yaml
state-archive-read-config:
  - object-store-config:
      object-store: "S3"
      # Use mysten-testnet-archives for testnet 
      # Use mysten-mainnet-archives for mainnet
      bucket: "mysten-<testnet|mainnet>-archives"
      # Use your AWS account access key id
      aws-access-key-id: "<AWS_ACCESS_KEY_ID>"
      # Use your AWS account secret access key
      aws-secret-access-key: "<AWS_SECRET_ACCESS_KEY>"
      aws-region: "us-west-2"
      object-store-connection-limit: 20
    # How many objects to read ahead when catching up  
    concurrency: 5
    # Whether to prune local state based on latest checkpoint in archive.
    # This should stay false for most use cases
    use-for-pruning-watermark: false
```

## Set up your own archival fallback

You can also set up the archival fallback to use your own S3 bucket. To do so, you must configure your Full node to 
archive transactions and effects. You can enable this by adding the following config to your `fullnode.yaml` file.

```yaml
state-archive-write-config:
  object-store-config:
    object-store: "S3"
    bucket: "<bucket_name>"
    aws-access-key-id: "<AWS_ACCESS_KEY_ID>"
    aws-secret-access-key: "<AWS_SECRET_ACCESS_KEY>"
    aws-region: "<aws_region>"
    object-store-connection-limit: 20
  concurrency: 5
  # This is needed to be set as true on the node that archives the data
  # This prevents the node from pruning its local state until the data has been
  # successfully archived
  use-for-pruning-watermark: true
```