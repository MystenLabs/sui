# Sui Rust SDK

This crate provides the Sui Rust SDK. For the crate documentation, see [https://docs.rs/sui_sdk/](https://docs.rs/sui_sdk/)

## Getting Started

We are currently working on publishing the Sui Rust SDK crate on crates.io. Until then, add the `sui-sdk` dependency as following:
```toml
sui-sdk = { git="https://github.com/mystenlabs/sui" }
tokio = { version = "1.2", features = ["full"] }
anyhow = "1.0"
```
<!-- Add the following dependency to your `Cargo.toml` file.  -->
<!-- 
```toml
sui_sdk = "0.1"
``` -->

The main building block for the Sui Rust SDK is the `SuiClientBuilder`, which provides a simple and straightforward way of connectiong to a Sui network and having access to the different available APIs. 

A simple example that connects to a running Sui local network and available Sui networks is shown below. If you want to try to run this program, make sure to spin up a local network with a local validator, a fullnode, and a faucet server (see [preqrequisites](README.md) for more inforrmation).

```rust
use sui_sdk::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("http://127.0.0.1:9000") // local network address
        .await?;
    println!("Sui local network version: {}", sui.api_version());

    // local Sui network, like the above one but using the dedicated function
    let sui_local = SuiClientBuilder::default().build_localnet().await?;
    println!("Sui local network version: {}", sui_local.api_version());

    // Sui devnet -- https://fullnode.devnet.sui.io:443
    let sui_devnet = SuiClientBuilder::default().build_devnet().await?;
    println!("Sui devnet version: {}", sui_devnet.api_version());

    // Sui testnet -- https://fullnode.testnet.sui.io:443
    let sui_testnet = SuiClientBuilder::default().build_testnet().await?;
    println!("Sui testnet version: {}", sui_testnet.api_version());

    Ok(())
}

```

# Rust SDK Examples

The [examples](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk/examples) folder provides both simple and advanced examples.
There are five files ending in `_api.rs` which provide code examples of the corresponding APIs and their methods. These showcase how to use the Sui Rust SDK, and can all be run locally against a local running Sui network. Below are instructions on the prerequisites and how to run these examples.  
## Preqrequisites

Unless otherwise specified, most of these examples assume that Sui is installed, and that there is a local network running.

* Install `sui` binary following the [Sui installation](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md##install-sui-binaries) docs.
* For local development, after the `sui` binary is installed, run `sui-test-validator` (or `cargo run --bin sui-test-validator` in the [cloned Git repository](https://github.com/mystenlabs/sui) if you don't want to install `Sui`) to spin up a local network with a local validator, a fullnode, and a faucet server. Refer to [this guide](https://docs.sui.io/build/sui-local-network) for more information. The local network will be up and running on `http://127.0.0.1:9000` and the faucet server on `http://127.0.0.1:9123`. 

## Running the examples

In the root folder of the `sui-sdk` crate, examples can be individually run using the command  `cargo run --example filename` (without `.rs` extension). For example:
* `cargo run --example sui_client`
* `cargo run --example coin_read_api`
* `cargo run --example event_api`
* `cargo run --example governance_api`
* `cargo run --example read_api`
* `cargo run --example transaction_builder_api`
## Basic Examples

### Connecting to Sui Network
The `SuiClientBuilder` struct provides a connection to the JSON-RPC Server and should be used for all read-only operations. The default URLs to connect to the Sui network are:

- Local: http://127.0.0.1:9000
- Devnet: https://fullnode.devnet.sui.io:443
- Testnet: https://fullnode.testnet.sui.io:443

For all available servers, see [here](https://sui.io/networkinfo). 

```rust
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("http://127.0.0.1:9000") // local network address
        .await?;
    println!("Sui local network version: {}", sui.api_version());

    // local Sui network, like the above one but using the dedicated function
    let sui_local = SuiClientBuilder::default().build_localnet().await?;
    println!("Sui local network version: {}", sui_local.api_version());

    // Sui devnet -- https://fullnode.devnet.sui.io:443
    let sui_devnet = SuiClientBuilder::default().build_devnet().await?;
    println!("Sui devnet version: {}", sui_devnet.api_version());

    // Sui testnet -- https://fullnode.testnet.sui.io:443
    let sui_testnet = SuiClientBuilder::default().build_testnet().await?;
    println!("Sui testnet version: {}", sui_testnet.api_version());

    Ok(())
}
```

### Reading the total balance for an address
```rust
use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::{ SuiClientBuilder};
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

   // local Sui network, like the above one but using the dedicated function
   let sui_local = SuiClientBuilder::default().build_localnet().await?;
   println!("Sui local network version: {}", sui_local.api_version());

   let active_address = SuiAddress::from_str("sui_address_here")?;

   // Total balance
   let total_balance = sui_local
      .coin_read_api()
      .get_balance(active_address, None)
      .await?;
   println!("Total balance for address: {active_address} is {}", total_balance);
   Ok(())
}
```

## Advanced Examples

See the transaction builder [example](examples/transaction_builder_api.rs).


## Games Examples

## Tic Tac Toe

### Demo quick start

#### 1. Prepare the environment 
   1. Install `sui` binary following the [Sui installation](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md##install-sui-binaries) docs.
   1. [Connect to Sui Devnet](https://github.com/MystenLabs/sui/blob/main/doc/src/build/connect-sui-network.md).
   1. [Make sure you have two addresses with gas](https://github.com/MystenLabs/sui/blob/main/doc/src/build/cli-client.md#add-existing-accounts-to-clientyaml) by using the `new-address` command to create new addresses:
      ```shell
      sui client new-address ed25519
      ```
      You must specify the key scheme, one of `ed25519` or `secp256k1` or `secp256r1`.
      You can skip this step if you are going to play with a friend. :)
   1. [Request Sui tokens](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md#sui-tokens) for all addresses that will be used to join the game.

#### 2. Publish the move contract
   1. [Download the Sui source code](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md#source-code).
   1. Publish the [`games` package](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/games) 
      using the Sui client:
      ```shell
      sui client publish --path /path-to-sui-source-code/sui_programmability/examples/games --gas-budget 10000
      ```
   1. Record the package object ID.
#### 3. Create a new tic-tac-toe game
   1. Run the following command in the Sui source code directory to start a new game, replacing the game package objects ID with the one you recorded:
      ```shell
      cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> new-game
      ```
        This will create a game for the first two addresses in your keystore by default. If you want to specify the identity of each player, 
use the following command and replace the variables with the actual player's addresses:
      ```shell
      cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> new-game --player-x <<player X address>> --player-o <<player O address>>
      ```
   1. Copy the game ID and pass it to your friend to join the game.
#### 4. Joining the game
Run the following command in the Sui source code directory to join the game, replacing the game ID and address accordingly:
```shell
cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> join-game --my-identity <<address>> --game-id <<game ID>>
```


# License
[SPDX-License-Identifier: Apache-2.0](https://github.com/MystenLabs/sui/blob/main/LICENSE) 