---
title: Create a local Sui network
---

Learn how to create a Sui network in your local environment. Use the [Sui Client CLI](cli-client.md) to interact with the local network.

## Genesis

The `genesis` command creates four validators and five user accounts
each with five gas objects. These are Sui [objects](../learn/objects.md) used
to pay for Sui [transactions](../learn/transactions.md#transaction-metadata),
such other object transfers or smart contract (Move) calls. These
numbers represent a sample configuration and have been chosen somewhat
arbitrarily; the process of generating the genesis state can be
customized with additional accounts, objects, code, etc. as described
in [Genesis customization](#customize-genesis).

1. Optionally, set `RUST_LOG=debug` for verbose logging.
1. Initiate `genesis`:
   ```shell
   $ sui genesis
   ```

All of this is contained in configuration and keystore files and an `authorities_db`
database directory. A `client_db` directory is also created upon running the
`sui client new-address` command covered later.

The network configuration is stored in `network.yaml` and can be used
subsequently to start the network. The `client.yaml` and `sui.keystore`
are also created to be used by the Sui client to manage the newly
created accounts.

By default, these files are placed in your home directory at
`~/.sui/sui_config` (created automatically if it does not yet exist). But you
can override this location by providing an alternative path to the `--working-dir`
argument. Run the command like so to place the files in the `dir` directory:

```shell
$ sui genesis --working-dir /path/to/sui/config/dir
```

> **Note:** That path and directory must already exist and will not be created with the `--working-dir` argument.

### Recreating genesis

To recreate Sui genesis state in the same location, which will remove
existing configuration files, pass the `--force` option to the `sui
genesis` command and either run it in the default directory (`~/.sui/sui_config`) or specify
it once again, using the `--working-dir` argument:

```shell
$ sui genesis --force --working-dir /path/to/sui/config/dir
```

## Client configuration

The genesis process creates a configuration file `client.yaml`, and a keystore file `sui.keystore` for the Sui client.  The config file contains information of the accounts and
the Sui Network server. The keystore file contains all the public-private key pairs of the created accounts.

Sui client uses the network information in `client.yaml` to communicate
with the Sui network validators and create transactions using the key
pairs residing in the keystore file.

The `accounts` section contains the account addresses that the client manages.

The `authorities` variable is part of the embedded gateway configuration. It contains
the Sui network validator's name, host and port information. It is used to establish connections
to the Sui network.

Note `send_timeout`, `recv_timeout` and `buffer_size` are the network
parameters, and `db_folder_path` is the path to the account's client state
database. This database stores all the transaction data, certificates
and object data belonging to the account.

#### Embedded gateway

As the name suggests, embedded gateway embeds the gateway logic into the application;
all data will be stored locally and the application will make direct
connection to the validators.

### Key management

The key pairs are stored in `sui.keystore`. However, this is not secure
and shouldn't be used in a production environment. We have plans to
implement more secure key management and support hardware signing in a future release.

:warning: **Do not use in production**: Keys are stored in file!

## Starting the network

Run the following command to start the local Sui network, assuming you
accepted the default location for configuration:

```shell
$ sui start
```

This command will look for the Sui network configuration file
`network.yaml` in the `~/.sui/sui_config` directory. But you can
override this setting by providing a path to the directory where
this file is stored:

```shell
$ sui start --config /path/to/sui/network/config/file
```

For example:

```shell
$ sui start --config /Users/name/tmp/network.yaml
```

Executing any of these two commands in a terminal window will result
in no output but the terminal will be "blocked" by the running Sui
instance (it will not return the command prompt). The command can
also be run in background.

NOTE: For logs, set `RUST_LOG=debug` before invoking `sui start`.

If you see errors when trying to start Sui network, particularly if you made some custom changes
 (e.g,
[customized client configuration](#client-configuration)), you should [recreate Sui genesis state](#recreating-genesis).
