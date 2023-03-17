---
title: Create a local Sui network
---

Learn how to create a Sui network in your local environment. Use the [Sui Client CLI](cli-client.md) to interact with the local network.

## Install Sui

To create a local Sui network, first install Sui. See [Install Sui to Build](install.md).

## Genesis

To create the configuration files and objects for a local Sui network, run the `genesis` command. Genesis creates the network configuration files in the ~/.sui/sui_config folder. This includes a YAML file for fullnode, network, client, and each validator. It also creates a sui.keystore that stores client key pairs. 

The network that genesis creates includes four validators and five user accounts that contain five coin objects each.

```shell
sui genesis
```

### Run genesis after using the Client CLI
If you used the Sui Client CLI before you create a local network, it created a client.yaml file in the .sui/sui_config directory. When you run genesis to create a local network, a warning displays that the .sui/sui_config folder is not empty because of the existing client.yaml file. You can use the `--force` argument to replace the configuration files, or use `--working-dir` to specify a different directory for the network configuration files.

Use the following command to replace the configuration files in the .sui/sui_config directory.
```shell
sui genesis --force
```

Use the following command to use a different directory to store the configuration files.
```shell
sui genesis --working-dir /workspace/config-files
```

The directory must already exist, and be empty, before you run the command.

#### Embedded gateway

You can use an embedded gateway with your local network. The gateway.yaml file contains information about the embedded gateway. The embedded gateway will be deprecated in a future release of Sui.

## Start the local network

Run the following command to start the local Sui network, assuming you
accepted the default location for configuration:

```shell
sui start
```

This command looks for the Sui network configuration file
`network.yaml` in the `~/.sui/sui_config` directory. If you used a different directory when you ran `genesis`, use the `--network.config` argument to specify the path to that directory when you start the network.

Use the following command to use a network.yaml file in a directory other than the default:

```shell
sui start --network.config /workspace/config-files/network.yaml
```
When you start the network, Sui generates an authorities_db directory that stores validator data, and a consensus_db directory that stores consensus data. These directories are created alongside the other configuration files, either in the default directory or where you specified the `--working-dir` to be when you ran `genesis`.

After the process completes, use the [Sui Client CLI](cli-client.md) to interact with the local network.

To test your apps against the latest changes or to prepare for new features ahead of the next DevNet/TestNet release, we recommend testing on a local network using the `sui-test-validator` binary. This binary starts a single-node cluster with full-node and faucet capabilities.

## Prerequisite

[Install](../build/install.md) the required libraries.

## Install Sui

You can install Sui from your local repository or from the remote repository on GitHub. If you build from your local source, you have the benefit of being able to run a local Sui Explorer and Sui Wallet.

To run from your local source, clone the repository locally (or get latest, if already cloned). Then, run `cargo build` from the `sui` directory:

```bash
# Clone the repository
git clone https://github.com/MystenLabs/sui.git
# Make sui the working directory
cd sui
# Build Sui
cargo build sui-test-validator sui
```

To use remote code, `cargo install` Sui from the GitHub repository:

```bash
# Change `--branch main` to `--branch devnet` or `--branch testnet` to 
# target different network versions
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch main sui-test-validator sui
```

## Running local network

To run a local network with validators and a faucet, open a Terminal or Console window at the `sui` root directory. Use the following command to run `sui-test-validator`, setting `RUST_LOG` to `consensus=off`:

```bash
RUST_LOG="consensus=off" cargo run --bin sui-test-validator
```

You can customize your local Sui network by passing values to the following flags:

```bash
OPTIONS:
        --epoch-duration-ms <EPOCH_DURATION_MS>
            The duration for epochs (defaults to one minute) [default: 60000]

        --faucet-port <FAUCET_PORT>
            Port to start the Sui faucet on [default: 9123]

        --fullnode-rpc-port <FULLNODE_RPC_PORT>
            Port to start the Fullnode RPC server on [default: 9000]
```

Use `sui-validator-test --help` to see these options in your console.

### Making faucet request

To get gas coins for an address, open a new Terminal or Conaole window or tab. Make a cURL request with the address you want to receive the coins. Use the `sui client active-address` command to get the current active address, if needed.

```bash
curl --location --request POST 'http://127.0.0.1:9123/gas' \
--header 'Content-Type: application/json' \
--data-raw '{
    "FixedAmountRequest": {
        "recipient": "0x<ADDRESS>"
    }
}'
```

If successful, the response resembles the following output:

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

### Accessing Full node

You can access your Full node using cURL:

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

If successful, the return resembles the following:

```bash
{
    "jsonrpc": "2.0",
    "result": 168,
    "id": 1
}
```

## Setup local Sui Explorer

While [https://explorer.sui.io/?network=local](https://explorer.sui.io/?network=local) is compatible with the local network, it might not have all the latest features that are available in the `main` branch of the Sui repository. To run `explorer` locally, open a Terminal or Console window in the `sui` directory (install [pnpm](https://pnpm.io/installation) first if you don't already have it installed):

```bash
pnpm explorer dev
```

After running the command, you can open a browser to [http://localhost:3000/](http://localhost:3000/) to access your local version of Sui Explorer.

For more details, see [https://github.com/MystenLabs/sui/tree/main/apps/explorer](https://github.com/MystenLabs/sui/tree/main/apps/explorer).

## Set up local Sui Wallet

Similar to local Sui Explorer, you can also setup a local Sui Wallet. Open a Terminal or Console window or tab at the `sui` root directory and use the `wallet start` command (install [pnpm](https://pnpm.io/installation) first if you don't already have it installed):

```bash
pnpm wallet start
```

For more details, reference [https://github.com/MystenLabs/sui/tree/main/apps/wallet](https://github.com/MystenLabs/sui/tree/main/apps/wallet).

## Generating example data

Open a Terminal or Console window at the `sui` root directory. From there, run the TypeScript SDK end to end test against the local network to generate example data to the network (install [pnpm](https://pnpm.io/installation) first if you don't already have it installed):

```bash
pnpm sdk test:e2e
```

For more details, refer to [https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing](https://github.com/MystenLabs/sui/tree/main/sdk/typescript#testing).
