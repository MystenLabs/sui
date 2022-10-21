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
   $ sui genesis
   ```

### Run genesis after using the Client CLI
If you used the Sui Client CLI before you create a local network, it created a client.yaml file in the .sui/sui_config directory. When you run genesis to create a local network, a warning displays that the .sui/sui_config folder is not empty because of the existing client.yaml file. You can use the `--force` argument to replace the configuration files, or use `--working-dir` to specify a different directory for the network configuration files.

Use the following command to replace the configuration files in the .sui/sui_config directory.
```shell
$ sui genesis --force
```

Use the following command to use a different directory to store the configuration files.
```shell
$ sui genesis --working-dir /workspace/config-files
```

The directory must already exist, and be empty, before you run the command.

#### Embedded gateway

You can use an embedded gateway with your local network. The gateway.yaml file contains information about the embedded gateway. The embedded gateway will be deprecated in a future release of Sui.

## Start the local network

Run the following command to start the local Sui network, assuming you
accepted the default location for configuration:

```shell
$ sui start
```

This command looks for the Sui network configuration file
`network.yaml` in the `~/.sui/sui_config` directory. If you used a different directory when you ran `genesis`, use the `--network.config` argument to specify the path to that directory when you start the network.

Use the following command to use a network.yaml file in a directory other than the default:

```shell
$ sui start --network.config /workspace/config-files/network.yaml
```
When you start the network, Sui generates an authorities_db directory that stores validator data, and a consensus_db directory that stores consensus data.

After the process completes, use the [Sui Client CLI](cli-client.md) to interact with the local network.