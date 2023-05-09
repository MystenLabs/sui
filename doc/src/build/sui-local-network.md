---
title: Create a local Sui network
---

Use a Sui local network to test your dApps against the latest changes to Sui, and to prepare for the next Sui release to the Devnet or Testnet network. To set up a local network, Sui provides the `sui-test-validator` binary. The `sui-test-validator` starts a local network that includes a Sui Full node, a Sui validator, and a Sui faucet. You can use the included faucet to get test SUI to use on the local network.

## Prerequisites

Install the necessary [prerequisites](../build/install.md#prerequisites) for Sui.

## Install Sui

Use the steps in this section to install the `sui-test-validator` to run a local network. To install Sui to build or for other purposes, use the steps in the [Install Sui](install.md) topic.

If you previously installed Sui, do one of the following:

- Use the same branch for the commands in this topic that you used to install Sui
- Install Sui again using the branch you intend to use for your local network

You have two options to install Sui:

- Clone the Sui GitHub repository locally, and then install Sui from your local drive
- Install Sui directly from the remote Sui repository.

If you clone the repository and install Sui from your local drive, you can also start a local Sui Explorer and Sui Wallet that works with your local network.

When you install `sui-test-validator` but don't have libpq installed, you might see the following message:

`ld: library not found for -lpq`

To resolve this, use Brew to install `libpq` with the following command:

```shell
brew install libpq
```

Also add the path to your profile:

```
export PATH="/opt/homebrew/opt/libpq/bin:$PATH"`
```

If you still have an issue, run the following command:

```shell
brew link --force libpq
```

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

The command uses the `main` branch of the Sui repository. To use a different branch, change the value for the `--branch` switch. For example, to use the `devnet` branch, specify `--branch devnet`.

## Start the local network

To start the local network, run the following command from the `sui` root folder.

```bash
RUST_LOG="consensus=off" cargo run --bin sui-test-validator
```

The command starts the `sui-test-validator`. The `RUST_LOG`=`consensus=off` turns off consensus for the local network.

**Important:** Each time you start the `sui-test-validator`, the network starts as a new network with no previous data. The local network is not persistent.

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

### Access your local Full node

Use the following command to retrieve the total transaction count from your local network:

```bash
curl --location --request POST 'http://127.0.0.1:9000' \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getTotalTransactionBlocks",
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

## Connect the Sui Client CLI to your local network

You can use the Sui Client CLI with any Sui network. By default it connects to Sui Devnet. To connect to your local network, create a new environment alias named `local` that sets the RPC URL the client uses to your local network.

```shell
sui client new-env --alias local --rpc http://127.0.0.1:9000
```

Next, use the following command to set the active environment to the new `local` environment you created.

```
sui client switch --env local
```

The command returns:

`Active environment switched to [local]`

You can check the current active environment with the following command:

```
sui client active-env
```

The command returns:

`local`

## Show the current active address

The Sui Client CLI uses the active address for command if you don't specify one. Use the following command to show the active address on your local network.

```
sui client active-address
```

The command returns an address:

`0xbc33e6e4818f9f2ef77d020b35c24be738213e64d9e58839ee7b4222029610de`

Use the active address to get test SUI to use on your local network. Use the `sui client addresses` command to see all of the addresses on your local network.

**Note:** The address returned when you run the command is unique and does not match the one used in this example.

## Use the local faucet

Transactions on your local network require SUI coins to pay for gas fees just like other networks. To send coins to a Sui Wallet connected to your local network, see [Set up a local Sui Wallet](#set-up-a-local-sui-wallet). You can use the address for the local Sui Wallet with the faucet.

Use the following cURL command to get test coins from the local faucet.

```bash
curl --location --request POST 'http://127.0.0.1:9123/gas' \
--header 'Content-Type: application/json' \
--data-raw '{
    "FixedAmountRequest": {
        "recipient": "0xbc33e6e4818f9f2ef77d020b35c24be738213e64d9e58839ee7b4222029610de"
    }
}'
```

If successful, the response resembles the following:

```
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

### Check the gas coin objects for the active address

After you get coins from the faucet, use the following command to view the coin objects for the address:

```shell
sui client gas
```

The response resembles the following, but with different IDs:

```
                             Object ID                              |  Gas Value
--------------------------------------------------------------------------------
 0x1d790713c1c3441a307782597c088f11230c47e609af2cec97f393123ea4de45 |  200000000
 0x20c1d5ad2e8693953fca09fd2fec0fbc52a787e0a0f77725220d36a09a5b312d |  200000000
 0x236714566110f5624516faa0da215ad29f8daa611e8b651d1e972168207567b2 |  200000000
 0xc81f30256bb04ad84bc4a92017cffd7c1f98286e028fa504d8515ad72ddd1088 |  200000000
 0xf61c8b21b305cc8e062b3a37de8c3a37583e17f437a449a2ab42321d019aeeb4 |  200000000

```

## Install Sui Wallet and Sui Explorer locally

To install and use the apps locally, you must first install [pnpm](https://pnpm.io/installation). Use the instructions appropriate for your operating system.

After you install `pnpm`, use the following command to install the required dependencies in your workspace:

```shell
pnpm install
```

After the installation completes, run the following command to install Sui Wallet and Sui Explorer:

```shell
pnpm turbo build
```

If you encounter an error from turbo build, confirm that there is no `package-lock.json`. If the file exists, remove it and then run the command again.

### Set up Sui Explorer on your local network

To connect the live Sui Explorer to your local network, open the URL:[https://suiexplorer.com/?network=local](https://suiexplorer.com/?network=local). The live version of Sui Explorer may not include recent updates added to the `main` branch of the Sui repo. To use Sui Explorer that includes the most recent updates, install and run Sui Explorer from your local clone of the Sui repo.

Run the following command from the `sui` root folder:

**Note:** To run the command you have `pnpm` installed. See [Install Sui Wallet and Sui Explorer locally](#install-sui-wallet-and-sui-explorer-locally) for details.

```bash
pnpm explorer dev
```

After the command completes, open your local Sui Explorer at the following URL: [http://localhost:3000/](http://localhost:3000/).

For more details about Sui Explorer, see the [Explorer README](https://github.com/MystenLabs/sui/blob/main/apps/explorer/README.md#set-up).

## Set up a local Sui Wallet

You can also use a local Sui Wallet to test with your local network. You can then see transactions executed from your local Sui Wallet on your local Sui Explorer.

**Note:** To run the command you must have `pnpm` installed. See [Install Sui Wallet and Sui Explorer locally](#install-sui-wallet-and-sui-explorer-locally) for details.

Before you start the Sui Wallet app, update its default environment to point to your local network. To do so, first make a copy of `sui/apps/wallet/configs/environment/.env.defaults` and rename it to `.env` in the same directory. In your `.env` file, edit the first line to read `API_ENV=local` and then save the file.

Run the following command from the `sui` root folder to start Sui Wallet on your local network:

```bash
pnpm wallet start
```

### Add local Sui Wallet to Chrome

After you build your local version of Sui Wallet, you can add the extension to Chrome:

1. Open a Chrome browser to `chrome://extensions`.
1. Click the **Developer mode** toggle to enable, if it's not already on.
1. Click the **Load unpacked** button and select your `sui/apps/wallet/dist` directory.

Consult the Sui Wallet [Readme](https://github.com/MystenLabs/sui/blob/main/apps/wallet/README.md#install-the-extension-to-chrome) for more information on working with a locally built wallet on Chrome.

## Generate example data

Use the TypeScript SDK to add example data to your network.

**Note:** To run the command you must complete the `Pre-requisites for Building Apps locally` section first.

Run the following command from the `sui` root folder:

```bash
pnpm sdk test:e2e
```

For additional information about example data for testing, see [https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing](https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing).

## Troubleshooting

If you do not use [Node.js 18](https://nodejs.org/de/blog/announcements/v18-release-announce), you might see the following message:

`Retrying requesting from faucet: Retry failed: fetch is not defined`

To resolve this, switch or update to Node.js 18 and then try again.

## Test with the Sui TypeScript SDK

The published version of the Sui TypeScript SDK might be an earlier version than the version of Sui you installed for your local network. To make sure you're using the latest version of the SDK, use the `experimental`-tagged version (for example, `0.0.0-experimental-20230317184920`) in the [Current Tags](https://www.npmjs.com/package/@mysten/sui.js/v/0.0.0-experimental-20230127130009?activeTab=versions) section of the Sui NPM registry.
