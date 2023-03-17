---
title: Sui Exchange Integration Guide
---

This topic describes how to integrate SUI, the token native to the Sui network, into a cryptocurrency exchange. The specific requirements and processes to implement an integration vary between exchanges. Rather than provide a step-by-step guide, this topic provides information about the primary tasks necessary to complete an integration. After the guidance about how to configure an integration, you can also find information and code samples related to staking on the Sui network.

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

Run the command in this section using the same branch of the repository for each. Replace `branch-name` with the branch you use. For example, use `devnet` to use the Sui Devnet network, or use `testnet` to use the Sui Testnet network. You must download all files to, and run all commands from, the same folder location.

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

Sui addresses do not require on-chain initialization, you can spend from an address if it corresponds to your private key. You can derive a 32-byte Sui address by hashing the signature scheme flag byte concatenated with public key bytes `flag || pubkey` using the [BLAKE2b](https://www.blake2.net/) (256 bits output) hashing function. 

Currently, Sui address supports these signature schemes: pure Ed25519, Secp256k1, Secp256r1 and Multisig. The corresponding flag bytes are 0x00, 0x01, 0x02, 0x03 respectively. 

The following code sample demonstrates how to derive a Sui address in Rust:

```rust
let flag = 0x00; // 0x00 = ED25519, 0x01 = Secp256k1, 0x02 = Secp256r1, 0x03 = Multisig
// Hash the [flag, public key] bytearray using Blake2b
let mut hasher = UserHash::default();
hasher.update([flag]);
hasher.update(pk);
let arr = hasher.finalize();
let sui_address_string = hex::encode(arr);
```

## Displaying addresses

Sui supports both addresses with and without a 0x prefix. Sui recommends that you always include the 0x prefix in API calls and when you display user addresses.

## Track balance changes for an address

You can track balance changes by calling `sui_getBalance` at predefined intervals. This call returns the total balance for an address. The total includes any coin or token type, but this document focuses on SUI. You can track changes in the total balance for an address between subsequent `sui_getBalance` requests.

The following bash example demonstrates how to use `sui_getBalance` for address 0x849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3. If you use a network other than Devnet, replace the value for `rpc` with the URL to the appropriate Full node.

```bash
rpc="https://fullnode.devnet.sui.io:443"
address="0x849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3"
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
use sui_sdk::{SuiClient, SuiClientBuilder};


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
   let sui = SuiClientBuilder::default().build(
      "https://fullnode.devnet.sui.io:443",
   ).await.unwrap();
   let address = SuiAddress::from_str("0x849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3")?;
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
address="0x849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3"
data="{\"jsonrpc\": \"2.0\", \"id\":1, \"method\": \"sui_getEvents\", \"params\": [{\"Recipient\": {\"AddressOwner\": \"0x849d63687330447431a2e76fecca4f3c10f6884ebaa9909674123c6c662612a3\"}}, null, null, true ]}"
curl -X POST -H 'Content-type: application/json' --data-raw "$data" $rpc
```

The response can include a large number of events. Add pagination to the response using the `nextCursor` key in the request. You can determine the corresponding `txDigest` and `eventSeq` from the `id` field of a transaction.

You can add the `txDigest` value instead of the first `null` within the `params`. The second `null` is an integer that defines how many results (up to 1000) to return and the `true` means ascending order. You can use the `nextCursor` so the response starts from a desired point.

The `id` field of any transaction looks like:
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
 * Sui creates checkpoints and adds finalized transactions. Note that transactions are finalized even before they are included in a checkpoint
 * Checkpoints do not fork, roll back, or reorganize.
 * Sui creates one checkpoint about every 3 seconds.

### Checkpoint API operations

Sui Checkpoint API operations include:
 * [sui_getCheckpoint](https://docs.sui.io/sui-jsonrpc#sui_getCheckpoint) - Retrieves the specified checkpoint.
 * [sui_getLatestCheckpointSequenceNumber](https://docs.sui.io/sui-jsonrpc#sui_getLatestCheckpointSequenceNumber) - Retrieves the sequence number of the most recently executed checkpoint.
 * sui_getCheckpoints - Retrieves a paginated list of checkpoints that occurred during the specified interval. Pending a future release.

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

## Signing Transactions

Please refer to [offline signing](https://github.com/MystenLabs/sui/blob/d0aceaea613b33fc969f7ca2cdd84b8a35e87de3/crates/sui/offline_signing.md) for more details on signature validity requirements.

A native weighted multi-sig multi-scheme signature is also supported. Please see [multisig](https://github.com/MystenLabs/sui/blob/d0aceaea613b33fc969f7ca2cdd84b8a35e87de3/crates/sui/multisig.md) for details. 

## SUI Staking and Delegation

The Sui blockchain uses a Delegated Proof-of-Stake mechanism (DPoS). This allows SUI token holders to stake their SUI tokens to any validator of their choice. When someone stakes their SUI tokens, it means those tokens are locked for the entire epoch. Users can withdraw their stake at any time, but new staking requests become active only at the start of the next epoch.

SUI holders who stake their tokens to validators earn rewards for helping secure the Sui network. Sui determines rewards for staking based on stake rewards on the network, and distributes them at the end of each epoch.

The total voting power in the Sui Network is always 10,000. The voting power of each individual validator is similar to basis points. For example, a voting power of 101 = 1.01%. Sui's quorum threshold (number of votes needed to confirm a transaction) is 6,667 (which is greater than 2/3). The voting power for a single validator is capped at 1,000 (10%) regardless of how much stake the validator has.

## Staking functions

Sui supports the following API operations related to staking. You can find the source code in the [sui_system](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/governance/sui_system.move) module.

 * `request_add_stake`
 Add delegated stake to a validator's staking pool.

```rust
public entry fun request_add_stake(
   self: &mut SuiSystemState,
   stake: Coin<SUI>,
   validator_address: address,
   ctx: &mut TxContext,
) {
   validator_set::request_add_stake(
       &mut self.validators,
       validator_address,
       coin::into_balance(stake),
       option::none(),
       ctx,
   );
}
```

 * `request_add_stake_mul_coin`
 Add delegated stake to a validator's staking pool using multiple coins.

```rust
public entry fun request_add_stake_mul_coin(
   self: &mut SuiSystemState,
   delegate_stakes: vector<Coin<SUI>>,
   stake_amount: option::Option<u64>,
   validator_address: address,
   ctx: &mut TxContext,
) {
   let balance = extract_coin_balance(delegate_stakes, stake_amount, ctx);
   validator_set::request_add_stake(&mut self.validators, validator_address, balance, option::none(), ctx);
}
```

 * `request_add_stake_with_locked_coin`
 Add delegated stake to a validator's staking pool using a locked SUI coin.

```rust
public entry fun request_add_stake_with_locked_coin(
   self: &mut SuiSystemState,
   stake: LockedCoin<SUI>,
   validator_address: address,
   ctx: &mut TxContext,
) {
   let (balance, lock) = locked_coin::into_balance(stake);
   validator_set::request_add_stake(&mut self.validators, validator_address, balance, option::some(lock), ctx);
}
```

 * `request_withdraw_stake`
 Withdraw some portion of a delegation from a validator's staking pool.

```rust
public entry fun request_withdraw_stake(
   self: &mut SuiSystemState,
   delegation: &mut Delegation,
   staked_sui: &mut StakedSui,
   principal_withdraw_amount: u64,
   ctx: &mut TxContext,
) {
   validator_set::request_withdraw_stake(
       &mut self.validators,
       delegation,
       staked_sui,
       principal_withdraw_amount,
       ctx,
   );
}
```

## Sui Exchange Integration FAQs

Get answers to common questions about Sui.

### How to change the amount of an existing stake?

During the staking period, you can add to or withdraw your stake from a validator. To modify your stake amount you can use the following functions:
 * Use the `request_add_stake` and `request_add_stake_with_locked_coin` methods to add to the staked amount.
 * Use the `request_withdraw_stake` method to withdraw your delegation.

### How is a staking transaction different from a typical transaction regarding construction, signing, and broadcasting?

Staking transactions are Move call transactions that call specific Move functions in the [sui_system](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/governance/sui_system.move) module of the Sui Framework. The staking transaction uses a shared object, and is no different from other shared object transactions.

### Is there a minimum and maximum staking amount (for validation and delegation)?

There will be a minimum amount required, as well as limits on stake changes within an epoch.

 * **Validation:** Requires a high minimum amount of SUI delegated with each validator to stay in the validator set.
 * **Delegation:** There will be a relatively low minimum amount for each delegation.

Specific amounts to be determined prior to Sui Mainnet.

### How to stake and un-stake SUI?

Sui Wallet supports both stake and un-staking. Staking via Move code or the Sui CLI is also possible - the relevant functions are in the [sui_system](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/governance/sui_system.move) module.

### Where are the Sui Developer Docs?

* Sui Documentation Portal: [https://docs.sui.io/](https://docs.sui.io/)
* Sui REST API's: [https://docs.sui.io/sui-jsonrpc](https://docs.sui.io/sui-jsonrpc)

### What is the difference between the devnet branch and the main branch of the Sui repo?

The main branch contains all the latest changes. The `devnet` branch reflects the binary that is currently running on the Devnet network.

### Can I get contract information through the RPC API?

Yes, contracts are also stored in objects. You can use the sui_getObject to fetch the object. Example: [https://explorer.sui.io/objects/0xe70628039d00d9779829bb79d6397ea4ecff5686?p=31](https://explorer.sui.io/objects/0xe70628039d00d9779829bb79d6397ea4ecff5686?p=31)

**Note:** You can see only the deserialized bytecode (as opposed to Source code).

### Can I get the information in the contract, such as the total amount of the currency issued and the number of decimal places?

There's no contract-level storage in Sui. In general, this contract-level information is usually stored in an object or event. For example, we store decimals in this object [https://github.com/MystenLabs/sui/blob/1aca0465275496e40f02a674938def962126412b/crates/sui-framework/sources/coin.move#L36](https://github.com/MystenLabs/sui/blob/1aca0465275496e40f02a674938def962126412b/crates/sui-framework/sources/coin.move#L36). And in this case we provide an [RPC endpoint](https://github.com/MystenLabs/sui/blob/main/crates/sui-json-rpc/src/api/).

### Is the gas price dynamic? Is it available through JSON-RPC?

Yes, the gas price is dynamic and exposed via the [sui_getReferenceGasPrice](https://docs.sui.io/sui-jsonrpc#sui_getReferenceGasPrice) endpoint.

### How can I delete an object within Sui?

You can delete objects (in most cases) only if the Move module that defines the object type includes a Move function that can delete the object, such as when a Move contract writer explicitly wants the object to be deletable.[https://docs.sui.io/devnet/build/programming-with-objects/ch2-using-objects#option-1-delete-the-object](https://docs.sui.io/devnet/build/programming-with-objects/ch2-using-objects#option-1-delete-the-object)

If the delete function is defined in the Move module, you can delete the object by invoking the Move call using CLI or wallet. Here’s an example:

 1. Create an example NFT using the Sui Client CLI: [https://docs.sui.io/devnet/build/cli-client#create-an-example-nft](https://docs.sui.io/devnet/build/cli-client#create-an-example-nft).

 2. Call this Move [function](https://github.com/MystenLabs/sui/blob/21c26ce6a5d4e3448abd74323e3164286d3deba6/crates/sui-framework/sources/devnet_nft.move#L69-L72) with the CLI by following [https://docs.sui.io/devnet/build/cli-client#calling-move-code](https://docs.sui.io/devnet/build/cli-client#calling-move-code).

### What is the denomination of Sui？

MIST is the smallest unit of a SUI Coin. 1 SUI equals 1 billion MIST, and 1 MIST equals 10^-9 of a SUI.

## Transactions FAQs

Questions about transaction in Sui.

### How can we subscribe to transaction events?

There are "Move events" that are emitted by Move code, and "transaction events" such as object transfers, creations, and deletions. See the [Sui Events](../build/event_api.md) topic for a list of all the events you can subscribe to via the pub/sub API and their structure.

### Can I get the corresponding transaction serial number through TransactionDigest?

As a best practice, don't rely on the transaction serial number because there's no total ordering of transactions on Sui. The transaction serial numbers differ between different Full nodes.

### Is the paged transaction data obtained by different nodes the same?

No, the ordering will be different on different nodes for now, while we are still working on checkpoints. After checkpoint process is complete, the ordering will be the same on all nodes

### Is there a nonce or timestamp mechanism for transactions?

There are no nonce or timestamps in our transaction data structure at the moment

### What is the transaction expiry window?

Transactions don't expire.

### How many validators will Sui have at Mainnet genesis?

The number is still under consideration. The validator set is not fixed, but validators must apply and then be approved through our validator application process.

### Is the address used for staking the same as the wallet address that owns the staked coins?

Yes, a user/validator stakes using the address that owns the staked coin. There is no special address derivation

### How is a staking transaction different from a typical transaction regarding construction, signing, and broadcasting?

Staking transactions are Move call transactions that call specific Move function in the [Sui Framework](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/governance/sui_system.move). The staking transaction uses a shared object, and is no different from other shared object transactions.

### Does Sui support staking a partial amount of the SUI owned by an address?

Yes, an address can own multiple coins of different amounts. Sui supports staking coins owned by an address to different validators. The minimum staking amount that can be delegated is 1 MIST which is equal to .000000001 SUI.

### Can I use one account address to stake with multiple validators?

Yes, if an address owns multiple coins, you can stake each coin with a different validator.

### Can I change the amount of an existing stake during the staking period?

Yes, you can add to or withdraw your stake from a validator. Use the following methods to modify the stake amount:

Use the [`request_add_stake`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui_system.md#0x2_sui_system_request_add_stake) and [`request_add_stake_with_locked_coin`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui_system.md#0x2_sui_system_request_add_stake_with_locked_coin) methods to add to the staked amount.

Use the [`request_withdraw_stake`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui_system.md#0x2_sui_system_request_withdraw_stake) method to withdraw all or part of the delegation.

### Does Sui require a bonding / warm-up period?

Yes, the specifics are still under consideration.

### Does Sui require an un-bonding / cool-down period?

Yes, the current un-bonding period is under consideration.

### Are staking rewards auto-compounded?

Yes, Sui uses a staking pool approach inspired by liquidity pools. Rewards are added to the pool and auto-compounded through the appreciation of pool token value relative to SUI tokens.

### Do rewards appear as inbound/outbound on-chain transactions?

Yes, rewards are added to the staking pool through a special system transaction at epoch boundaries.

### How long does it take to get the first reward after staking? How frequently are rewards paid out?

Rewards are compounded every epoch, and paid out when you withdraw your stake. You must stake for the entire duration of an epoch to receive rewards for that epoch.

### How does slashing work, and what are the penalties?

There will not be slashing for the principal stake allocated. Instead, validators will get penalized by having fewer future rewards when these get paid out. Rewards that have already been accrued are not at risk.

### Does Sui support on-chain governance or voting?

On-chain governance is not implemented for Sui. There is no plan to add it in the near future.

### How can I retrieve the current block height or query a block by height using a Sui endpoint?

Sui is [DAG](https://cointelegraph.com/explained/what-is-a-directed-acyclic-graph-in-cryptocurrency-how-does-dag-work)-based, so the block-based view of the transaction history is not always the most direct one. To get the latest transaction, use the Transaction Query API:

    ```json
    {
      "jsonrpc": "2.0",
      "id": 1,
      "method": "sui_queryTransactions",
      "params": [
        "All",
        <last known transaction digest>,
        100,
        "Ascending"
      ]
    }
    ```

### How are transactions proposed by validators if they're not included in blocks? Does a validator propose blocks or just individual transactions?

Validators form a certificate (a quorum of signatures) for each transaction, and then propose checkpoints consisting of certificates since the last checkpoint. You can read more in section 4.3 of the [Sui Smart Contract Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf).

### How do I get test Devnet coins?

- You can find our [faucet in Discord](https://discord.com/channels/916379725201563759/971488439931392130). You can also request coins from the [Sui Faucet](../build/faucet.md) programmatically.

### How can I get in touch and request more information?

- Please visit our [Discord server](https://discord.gg/sui).

