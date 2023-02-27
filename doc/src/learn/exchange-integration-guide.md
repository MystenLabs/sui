---
title: Sui Exchange Integration Guide
---

This topic describes how to integrate SUI, the token native to the Sui network, into a cryptocurrency exchange. The specific requirements and processes to implement an integration vary between exchanges. Rather than provide a step-by-step guide, this topic provides information about the primary tasks necessary to complete an integration. After the guidance about how to configure an integration, you can also find information and code samples related to staking and delegation on the Sui network.

## Requirements to configure a SUI integration

The requirements to configure a SUI integration include:
 * A Sui Full node. You can operate your own Sui Full node or use a Full node from a node operator.
 * Suggested hardware requirements to run a Sui Full node:
    * CPU: 10 core
    * RAM: 32 GB
    * Storage: 1 TB SSD

We recommend running Sui Full nodes on Linux. Sui supports the Ubuntu and Debian distributions.

## Configure a Sui Full node

You can set up and configure a Sui Full node using Docker or directly from source code in the Sui GitHub repository.

### Install a Sui Full node using Docker

Run the command in this section using the same branch of the repository for each. Replace `branch-name` with the branch you use. For example, use `devnet` to use the Sui Devnet network, or use `testnet` to use the Sui Testnet network.

 1. Install [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/). Docker Desktop version installs Docker Compose.
 1. Install dependencies for Linux:
    ```bash
    apt update \
    && apt install -y --no-install-recommends \ 
    tzdata \ 
    ca-certificates \ 
    build-essential \ 
    pkg-config \ 
    cmake
    ```
 1. Download the docker-compose.yaml file:
    ```bash
    wget https://github.com/MystenLabs/sui/blob/branch-name/docker/fullnode/docker-compose.yaml
    ```
 1. Download the fullnode-template.yaml file:
    ```bash
    wget https://github.com/MystenLabs/sui/raw/branch-name/crates/sui-config/data/fullnode-template.yaml
    ```
 1. Download the genesis.blob file:
    ```bash
    wget https://github.com/MystenLabs/sui-genesis/raw/main/branch-name/genesis.blob
    ```
 1. Start the Full node. The -d switch starts it in the background (detached mode).
    ```bash
    docker-compose up -d
    ```

## Install a Sui Full node from source

Use the steps in this section to install and configure a Sui Full node directly from the Sui GitHub repository. These steps use [Cargo](https://doc.rust-lang.org/cargo/), the Rust package manager.

 1. Install prerequisites for Sui.
 1. Clone the Sui repository:
    ```bash
    git clone https://github.com/MystenLabs/sui.git -b branch-name
    ```
    Replace `branch-name` with the branch to use. You should use the same branch for all commands.
 1. Change directories to /sui:
    ```bash
    cd sui
    ```
 1. Copy the fullnode.yaml template:
    ```bash
    cp crates/sui-config/data/fullnode-template.yaml fullnode.yaml
    ```
 1. Download the genesis.blob file:
    ```bash
    wget https://github.com/MystenLabs/sui-genesis/raw/main/branch-name/genesis.blob
    ```
    Change branch-name to the same branch you used for previous commands.
 1. Optionally, if you installed Sui to a path other than the default, modify the fullnode.yaml file to use the path you used. Update the path to the folder where you installed sui-fullnode for the `db-path` and `genesis-file-location` as appropriate:
    `db-path: "/db-files/sui-fullnode-folder"`
    `genesis-file-location: "/sui-fullnode-folder/genesis.blob"`
 1. Start you Sui Full node:
    ```bash
    cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```
## Set up Sui addresses

Sui addresses do not require on-chain initialization, you own an address if you own the key for the address. You can derive the Sui address by hashing the signature flag byte + public key bytes. The following code sample demonstrates how to derive a Sui address in Rust:

```rust
let flag = 0x00; // 0x00 = ED25519, 0x01 = Secp256k1, 0x02 = Secp256r1
// Hash the [flag, public key] bytearray using SHA3-256
let mut hasher = Sha3_256::default();
hasher.update([flag]);
hasher.update(pk);
let g_arr = hasher.finalize();


// The first 32 bytes is the Sui address.
let mut res = [0u8; SUI_ADDRESS_LENGTH]; // SUI_ADDRESS_LENGTH = 32
res.copy_from_slice(&AsRef::<[u8]>::as_ref(&g_arr)[..SUI_ADDRESS_LENGTH]);
let sui_address_string = hex::encode(res);
```

## Track balance changes for an address

You can track balance changes by calling `sui_getBalance` at predefined intervals. This call returns the total balance for an address. The total includes any coin or token type, but this document focuses on SUI. You can track changes in the total balance for an address between subsequent `sui_getBalance` requests.

The following bash example demonstrates how to use `sui_getBalance` for address 0xa38bc2aa63c34e37821f7abb34dbbe97b7ab2ea2. If you use a network other than Devnet, replace the value for `rpc` with the URL to the appropriate Full node.

```bash
rpc="https://fullnode.devnet.sui.io:443"
address="0xa38bc2aa63c34e37821f7abb34dbbe97b7ab2ea2"
data="{\"jsonrpc\": \"2.0\", \"method\": \"sui_getBalance\", \"id\": 1, \"params\": [\"$address\"]}"
curl -X POST -H 'Content-type: application/json' --data-raw "$data" $rpc
```

The response is a JSON object that includes the totalBalance for the address:
```json
{
  "jsonrpc":"2.0",
  "result":{
     "coinType":"0x2::sui::SUI",
     "coinObjectCount":40,
     "totalBalance":10000000000,
     "lockedBalance":{
       
     }
  },
  "id":1
}
```

The following example demonstrates using sui_getBalance in Rust:
```rust
use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClient;


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
   let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", None, None).await?;
   let address = SuiAddress::from_str("0xa38bc2aa63c34e37821f7abb34dbbe97b7ab2ea2")?;
   let objects = sui.read_api().get_balance(address).await?;
   println!("{:?}", objects);
   Ok(())
}
```

## Use events to track balance changes for an address

You can also track the balance for an address by subscribing to all of the events emitted from it. Use a filter to include only the events related to SUI coins, such as when the address acquires a coin or pays for a gas fee.
The following example demonstrates how to filter events for an address using bash and cURL:

```bash
rpc="https://fullnode.devnet.sui.io:443"
address="0xa38bc2aa63c34e37821f7abb34dbbe97b7ab2ea2"
data="{\"jsonrpc\": \"2.0\", \"id\":1, \"method\": \"sui_getEvents\", \"params\": [{\"Recipient\": {\"AddressOwner\": \"0xa38bc2aa63c34e37821f7abb34dbbe97b7ab2ea2\"}}, null, null, true ]}"
curl -X POST -H 'Content-type: application/json' --data-raw "$data" $rpc
```

The response can include a large number of events. Add pagination to the response using the `nextCursor` key. You can determine the corresponding `txDigest` and `eventSeq` from the id field of a transaction.  

You can add the value instead of the first null inside the params. The second null is an integer that defines how many results (up to 1000) to return and the `true` means ascending order. By using the `nextCursor` the reply can start from a desired point.

The “id” field of any transaction looks like:
```bash
"id": {
         "txDigest": "GZQN9pE3Zr9ZfLzBK1BfVCXtbjx5xKMxPSEKaHDvL3E2",
         "eventSeq": 6019
       }
```

With this data, create a nextCursor as follows:
```bash
nextCursor : {"txDigest": "GZQN9pE3Zr9ZfLzBK1BfVCXtbjx5xKMxPSEKaHDvL3E2","eventSeq": 6019}
```

## Blocks vs Checkpoints

Sui is a DAG-based blockchain and uses checkpoints for node synchronization and global transaction ordering. Checkpoints differ from blocks in the following ways:
 * Sui creates checkpoints and adds transaction after finality
 * Checkpoints do not fork, roll back, or reorganize.
 * Sui creates one checkpoint about every 3 seconds.

### Checkpoint API operations

Sui Checkpoint API operations include:
 * sui_getCheckpoint - Retrieves the specified checkpoint.
 * sui_getLatestCheckpointSequenceNumber - Retrieves the sequence number of the most recently executed checkpoint.
 * sui_getCheckpoints - Retrieves a paginated list of checkpoints that occurred during the specified interval.

## SUI Balance transfer

To transfer a specific amount of SUI between addresses, you need a SUI token object with that specific value. In Sui, everything is an object, including SUI tokens. The amount of SUI in each SUI token object varies. For example, an address could own 3 SUI tokens with different values: one of 0.1 SUI, a second of 1.0 SUI, and a third with 0.005 SUI. The total balance for the address equals the sum of the values of the individual SUI token objects, in this case, 1.105 SUI.

You can merge and split SUI token objects to create token objects with specific values. To create a SUI token worth .6 SUI, split the token worth 1 SUI into two token objects worth .6 SUI and .4 SUI. 

To transfer a specific amount of SUI, you need a SUI token worth that specific amount. To get a SUI token with that specific value, you might need to split or merge existing SUI tokens. Sui supports several methods to accomplish this, including some that do not require you to manually split or merge coins.

## Sui API operations for transfers

Sui supports the following API operations related to transferring SUI between addresses:

 * [sui_transferObject](https://docs.sui.io/sui-jsonrpc#sui_transferObject) 
   Because SUI tokens are objects, you can transfer SUI tokens just like any other object. This method requires a gas token, and is useful in niche cases only.

 * [sui_payAllSui](https://docs.sui.io/sui-jsonrpc#sui_payAllSui)
   This method accepts an array of SUI token IDs. It merges all existing tokens into one, deducts the gas fee, then sends the merged token to the recipient address.

   The method is especially useful if you want to transfer all SUI from an address. To merge together all coins for an address, set the recipient as the same address. This is a native Sui method so is not considered a transaction in Sui.

 * [sui_paySui](https://docs.sui.io/sui-jsonrpc#sui_paySui)
   This operation accepts an array of SUI token IDs, an array of amounts, and an array of recipient addresses.

   The amounts and recipients array map one to one. Even if you use only one recipient address, you must include it for each amount in the amount array.

   The operation merges all of the tokens provided into one token object and settles the gas fees. It then splits the token according to the amounts in the amounts array and sends the first token to the first recipient, the second token to the second recipient, and so on. Any remaining SUI on the token stays in the source address.

   The benefits of this method include: no gas fees for merging or splitting tokens, and the abstracted token merge and split. The `sui_paySui` operation is a native function, so the merge and split operations are not considered Sui transactions. The gas fees for them match typical transactions on Sui.You can use this operation to split coins in your own address by setting the recipient as your own address. Note that the total value of the input coins must be greater than the total value of the amounts to send.

 * [sui_pay](https://docs.sui.io/sui-jsonrpc#sui_pay)
   This method is similar to sui_paySui, but it accepts any kind of coin or token instead of only SUI. You must include a gas token, and all of the coins or tokens must be the same type.

 * [sui_transferSui](https://docs.sui.io/sui-jsonrpc#sui_transferSui)
    This method accepts only one SUI token object and an amount to send to the recipient. It uses the same token for gas fees, so the amount to transfer must be strictly less than the value of the SUI token used.

## SUI Staking and Delegation

The Sui blockchain uses a delegated Proof-of-Stake mechanism (DPoS). This allows SUI token holders to delegate their tokens to any validator of their choice. When someone delegates their SUI tokens, it means those tokens are locked for the entire epoch. Users can withdraw their stake and stake with a different validator between epochs.

SUI holders who delegate their tokens to validators earn rewards for helping secure the Sui  network. Sui determines rewards for delegation based on stakw rewards on the network, and distributes them at the end of each epoch.

The total stake delegated to a validator determines the validator’s voting power.

We're finalizing details for Sui staking and delegation and will update the documentation when available.



