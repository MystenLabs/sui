---
title: Create a local Sui network
---

Use a Sui local network to test your dApps against the latest changes to Sui, and to prepare for the next Sui release to the Devnet or Testnet network. To set up a local network, Sui provides the `sui-test-validator` binary. The `sui-test-validator` starts a local network that includes a Sui Full node, a Sui validator, and a Sui faucet. You can use the included faucet to get test SUI to use on the local network.

## Prerequisites

Install the necessary [prerequisites](../build/install.md#prerequisites) for Sui.

## Install Sui

Use the steps in this section to install the `sui-test-validator` to run a local network. To install Sui to build or for other purposes, use the steps in the [Install Sui](install.md) topic.

If you previously installed Sui, do one of the following:
 * Use the same branch for the commands in this topic that you used to install Sui
 * Install Sui again using the branch you intend to use for your local network

You have two options to install Sui:
 * Clone the Sui GitHub repository locally, and then install Sui from your local drive
 * Install Sui directly from the remote Sui repository.

 If you clone the repository and install Sui from your local drive, you can also start a local Sui Explorer and Sui Wallet that works with your local network.

### Install Sui locally

Use the following command to clone the Sui repo, change to the Sui folder after the clone completes, and then use cargo to install the `sui-test-validator` and `sui` binaries from your local drive.

```bash
# Clone the repository
git clone https://github.com/MystenLabs/sui.git
# Make sui the working directory
cd sui
# Build Sui
cargo build --bin sui-test-validator --bin sui
```

### Install Sui from GitHub

Use the following command to install Sui directly from the Sui GitHub repository:

```bash
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch main sui-test-validator sui
```

Note that the command uses the `main` branch of the Sui repository. To use a different branch, change the value for the `--branch` switch. For example, to use the `devnet` branch, specify `--branch devnet`.

## Start the local network

To start the local network, run the following command from the `sui` root folder.

```bash
RUST_LOG="consensus=off" cargo run --bin sui-test-validator
```

The command starts the `sui-test-validator`. The `RUST_LOG`=`consensus=off` turns off consensus for the local network.

**Important** Each time you start the `sui-test-validator`, the network starts as a new network with no previous data. The local network is not persistent.

To customize your local Sui network, such as changing the port used, include additional parameters in the command to start `sui-test-validator`:

```
OPTIONS:
        --epoch-duration-ms <EPOCH_DURATION_MS>
            The duration for epochs (defaults to one minute) [default: 60000]

        --faucet-port <FAUCET_PORT>
            Port to start the Sui faucet on [default: 9123]

        --fullnode-rpc-port <FULLNODE_RPC_PORT>
            Port to start the Fullnode RPC server on [default: 9000]
```

Use `sui-validator-test --help` to see these options in your console.

## Use the local faucet

You need to have coins to pay for gas on your local network just like other networks. Use the following cURL command to get test coins from the local faucet you just installed and started. 

To add the coins to the current active address on the local network, use the `sui client active-address` command to retrieve it. Use the `sui client addresses` command to see all of the addresses on your local network. To send coins to a Sui Wallet connected to your local network, see [Set up a local Sui](#set-up-a-local-sui-wallet).

```bash
curl --location --request POST 'http://127.0.0.1:9123/gas' \
--header 'Content-Type: application/json' \
--data-raw '{
    "FixedAmountRequest": {
        "recipient": "0x<ADDRESS>"
    }
}'
```

If successful, the response resembles the following:

```bash
{
    "transferredGasObjects": [
        {
            "amount": 200000000,
            "id": "0x192ce62506ed8705b76e8423be1f6e011064a3f887ba924605f27a8c83c8c970",
            "transferTxDigest": "7sp4fFPH2WaUgvN43kjDzCpEhKfifqjx5RTki74y8T3E"
        },
        {
            "amount": 200000000,
            "id": "0x31d003ade00675d1ab82b225bfcceaa60bb993f5d90e9d0aa88f81dc24ec14d6",
            "transferTxDigest": "7sp4fFPH2WaUgvN43kjDzCpEhKfifqjx5RTki74y8T3E"
        },
        {
            "amount": 200000000,
            "id": "0x98cbdc93ae672110f91bc0c39c0c87bc66f36984c79218bb2c0bac967260970c",
            "transferTxDigest": "7sp4fFPH2WaUgvN43kjDzCpEhKfifqjx5RTki74y8T3E"
        },
        {
            "amount": 200000000,
            "id": "0xba66aee6289cc6d0203c451bea442ad30d4cfe699e50b36fed0ff3e99ba51529",
            "transferTxDigest": "7sp4fFPH2WaUgvN43kjDzCpEhKfifqjx5RTki74y8T3E"
        },
        {
            "amount": 200000000,
            "id": "0xd9f0b521443d66227eddc2aac2e16f667ca9caeef9f1b7afb4a6c2fc7dcb58d8",
            "transferTxDigest": "7sp4fFPH2WaUgvN43kjDzCpEhKfifqjx5RTki74y8T3E"
        }
    ],
    "error": null
}
```

### Access your local Full node

Use the following command to retrieve the total transaction count from your local network:

```bash
curl --location --request POST 'http://127.0.0.1:9000' \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getTotalTransactionNumber",
  "params": []
}'
```

If successful, the response resembles the following:

```bash
{
    "jsonrpc": "2.0",
    "result": 168,
    "id": 1
}
```

## Set up a local Sui Explorer

To connect the live Sui Explorer to your local network, open the URL:[https://explorer.sui.io/?network=local](https://explorer.sui.io/?network=local). The live version of Sui Explorer may not include recent updates added to the `main` branch of the Sui repo. To use Sui Explorer that includes the most recent updates, install and run Sui Explorer from your local clone of the Sui repo.

**Note:** To run the command you must have [pnpm](https://pnpm.io/installation) installed.

Run the following command from the `sui` root folder:

```bash
pnpm explorer dev
```

After the command completes, open your local Sui Explorer at the following URL: [http://localhost:3000/](http://localhost:3000/).

For more details about Sui explorer, see [https://github.com/MystenLabs/sui/tree/main/apps/explorer](https://github.com/MystenLabs/sui/tree/main/apps/explorer).

## Set up a local Sui Wallet

You can also use a local Sui Wallet to test with your local network. You can then see transactions executed from your local Sui Wallet on your local Sui Explorer.

**Note:** To run the command you must have [pnpm](https://pnpm.io/installation) installed.

Run the following command from the `sui` root folder to start Sui Wallet on your local network:

```bash
pnpm wallet start
```

**Note** You can set the default environment for the wallet to use so that you don't have to switch network manually. For details, see [https://github.com/MystenLabs/sui/tree/main/apps/wallet#environment-variables](https://github.com/MystenLabs/sui/tree/main/apps/wallet#environment-variables). 

## Generate example data

Use the TypeScript SDK to add example data to your network. 

**Note:** To run the command you must have [pnpm](https://pnpm.io/installation) installed.

Run the following command from the `sui` root folder: 

```bash
pnpm sdk test:e2e
```

For additional information about example data for testing, see [https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing](https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing).

## Test with the Sui TypeScript SDK

The published version of the Sui TypeScript SDK might be an earlier version than the version of Sui you installed for your local network. To make sure you're using the latest version of the SDK, use the `experimental`-tagged version (for example, `0.0.0-experimental-20230317184920`) in the [Current Tags](https://www.npmjs.com/package/@mysten/sui.js/v/0.0.0-experimental-20230127130009?activeTab=versions) section of the Sui NPM registry.
