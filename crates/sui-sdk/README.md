# Rust SDK examples

Examples of interacting with the move contract using the Sui Rust SDK.

## Tic Tac Toe

### Demo quick start

#### 1. Prepare the environment 
   * Install `sui` and `rpc-server` binaries following the [installation doc](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md#binaries).
   * [Connect to devnet](https://github.com/MystenLabs/sui/blob/main/doc/src/build/cli-client.md#connect-to-devnet).
   * Make sure you have two addresses with gas, you can use the new-address command to create new addresses `sui client new-address`, 
   you can skip this step if you are going to play with a friend :)
   * [Request gas tokens](https://github.com/MystenLabs/sui/blob/main/doc/src/explore/devnet.md#request-gas-tokens) for all addresses that will be used to join the game.

#### 2. Publish the move contract
   * [Download the Sui source code](https://github.com/MystenLabs/sui/blob/main/doc/src/build/install.md#source-code)
   * Publish the [`games` package](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/games) 
      using the Sui client and copy down the package object ID.
      ```shell
      sui client publish --path /path-to-sui-source-code/sui_programmability/examples/games --gas-budget 10000
      ```
#### 3. Create a new tic-tac-toe game
   * run the following command in the sui source code folder to start a new game.
      ```shell
      cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> new-game
      ```
        this will create a game for the first two addresses in your keystore by default. If you want to specify the identity of each player, 
you can use the following command
      ```shell
      cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> new-game --player-x <<player X address>> --player-o <<player O address>>
      ```
   * Copy the game id and pass it to your friend to join the game.
#### 4. Joining the game
   * run the following command in the sui source code folder to join the game.
      ```shell
      cargo run --example tic-tac-toe -- --game-package-id <<games package object ID>> join-game --my-identity <<address>> --game-id <<game ID>>
      ```