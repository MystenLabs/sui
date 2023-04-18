# Rust SDK examples

This directory contains examples of interacting with a Move language smart contract using the Sui Rust SDK. See the [introduction to the Rust SDK](https://github.com/MystenLabs/sui/blob/main/doc/src/build/rust-sdk.md) for additional details.

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
