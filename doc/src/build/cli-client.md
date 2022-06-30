---
title: Sui CLI Quick Start
---

Welcome to the Sui tutorial on the Sui CLI developed
to facilitate experimentation with Sui features using a
command line interface. In this document, we describe how to set up
the Sui client and execute commands through its command line
interface, *Sui CLI*.

## Set up

Follow the instructions to [install Sui binaries](install.md#binaries).

## Connect to DevNet
We are hosting a public [DevNet](../explore/devnet.md) for the community to
experiment with our  tech and help to shape the future of the Sui network. To
connect the Sui  client to the DevNet, run the following command:
```shell
$ sui client
```
The Sui CLI will print the following line if the client is starting up the
first time.
```shell
Config file ["/Users/dir/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui RPC server [y/n]?
```
Type 'y' and then press 'Enter'. You should see the following output:
```shell
Sui RPC server Url (Default to Sui DevNet if not specified) :
```
The Sui client will prompt for the RPC server URL; press 'Enter' and it will default to DevNet.
Or enter a custom URL if you want to connect to a server hosted elsewhere.

If you have used the Sui client before with a local network, follow the next section to
[manually change the RPC server URL](#manually-change-the-rpc-server-url) to DevNet.

### Manually change the RPC server URL
If you have used the Sui client before, you will have an existing `client.yaml` configuration
file. Change the configured RPC server URL to DevNet by using:
```shell
$ sui client switch --gateway https://gateway.devnet.sui.io:443
```

## Genesis

The `genesis` command creates four validators and five user accounts
each with five gas objects. These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
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

The genesis process creates a configuration file `client.yaml`, and a keystore file `sui.keystore` for the
Sui client.  The config file contains information of the accounts and
the Sui Network server. The keystore file contains all the public-private key pairs of the created accounts.
Sui client uses the network information in `client.yaml` to communicate
with the Sui network validators  and create transactions using the key
pairs residing in the keystore file.

Here is an example of `client.yaml` showing the accounts and key pairs
in the client configuration (with some values omitted):

```yaml
---
accounts:
  - b02b5e57fe3572f94ad5ac2a17392bfb3261f7a0
  - b4f5ed3cbe78c7969e6ac073f9a0c525fd07f05a
  - 48ff0a932b12976caec91d521265b009ad5b2225
  - 08da15bee6a3f5b01edbbd402654a75421d81397
  - 3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
keystore:
  File: /Users/user/.sui/sui_config/sui.keystore
gateway:
  embedded:
    epoch: 0
    validator_set:
      - public-key: Ot3ov659M4tl59E9Tq1rUj5SccoXstXrMhQSJX7pFKQ=
        stake: 1
        network-address: /dns/localhost/tcp/57468/http
      - public-key: UGfB4wzJ2Lntn+WJvv+83RSigpuf7Vv2AmCPQR28TVY=
        stake: 1
        network-address: /dns/localhost/tcp/57480/http
      - public-key: 5bO8DUgmA9i1SiUka5BT6VjIclMNQBRnbVww2IXxFqw=
        stake: 1
        network-address: /dns/localhost/tcp/57492/http
      - public-key: 8uV0ml/DPUXG9UbrnlP6v08XaBum9pcIDelRT04NanU=
        stake: 1
        network-address: /dns/localhost/tcp/57504/http
    send_timeout:
      secs: 4
      nanos: 0
    recv_timeout:
      secs: 4
      nanos: 0
    buffer_size: 650000
    db_folder_path: /Users/user/.sui/sui_config/client_db
active_address: "0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0"
```

The `accounts` variable contains the account's address that the client manages. The
`gateway` variable contains the information of the Sui network that the client will
be connecting to.

The `authorities` variable is part of the embedded gateway configuration. It contains
the Sui network validator's name, host and port information. It is used to establish connections
to the Sui network.

Note `send_timeout`, `recv_timeout` and `buffer_size` are the network
parameters, and `db_folder_path` is the path to the account's client state
database. This database stores all the transaction data, certificates
and object data belonging to the account.

### Sui Network Gateway

The Sui Network Gateway (or simply, Sui Gateway) is an abstraction layer that acts as the entry
point to the Sui network. Different gateway implementations can be used by the application layer
based on their use cases.

#### Embedded gateway

As the name suggests, embedded gateway embeds the gateway logic into the application;
all data will be stored locally and the application will make direct
connection to the validators.

#### RPC gateway
You can also connect the client to the Sui network via an [RPC Gateway](json-rpc.md#start-local-rpc-server);
To use the RPC gateway, update `client.yaml`'s `gateway` section to:
```yaml
...
gateway:
  rpc: "http://localhost:5001"
...
```

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

## Using the Sui client

Now start a new terminal since you have the Sui network running in the first terminal.

The following commands are supported by the Sui client:

    active-address        Default address used for commands when none specified
    addresses             Obtain the Addresses managed by the client
    call                  Call Move function
    clear                 Clear screen
    create-example-nft    Create an example NFT
    echo                  Write arguments to the console output
    env                   Print environment
    exit                  Exit the interactive shell
    gas                   Obtain all gas objects owned by the address
    help                  Print this message or the help of the given subcommand(s)
    history               Print history
    merge-coin            Merge two coin objects into one coin
    new-address           Generate new address and keypair
    object                Get obj info
    objects               Obtain all objects owned by the address
    publish               Publish Move modules
    split-coin            Split a coin object into multiple coins
    switch                Switch active address and network(e.g., devnet, local rpc server)
    sync                  Synchronize client state with authorities
    transfer-coin         Transfer coin object
    transfer-sui          Transfer SUI, and pay gas with the same SUI coin object. If amount is
                              specified, only the amount is transferred; otherwise the entire object
                              is transferred

> **Note:** The `clear`, `echo`, `env` and `exit` commands exist only in the interactive shell.

Use `sui client -h` to see the most up-to-date list of commands.

Use `help <command>` to see more information on each command.

You can start the client in two modes: interactive shell or command line interface.

### Interactive shell

To start the interactive shell, execute the following (in a different
terminal window than one used to execute `sui start`). Assuming you
accepted the default location for configuration:

```shell
$ sui console 
```

This command will look for the client configuration file
`client.yaml` in the `~/.sui/sui_config` directory. But you can
override this setting by providing a path to the directory where this
file is stored:

```shell
$ sui console --config /path/to/client/config/file
```

The Sui interactive client console supports the following shell functionality:

* *Command history* -
  The `history` command can be used to print the interactive shell's command history;
  you can also use Up, Down or Ctrl-P, Ctrl-N to navigate previous or next matches from history.
  History search is also supported using Ctrl-R.
* *Tab completion* -
  Tab completion is supported for all commands using Tab and Ctrl-I keys.
* *Environment variable substitution* -
  The Sui console will substitute inputs prefixed with `$` with environment variables,
  you can use the `env` command to print out the entire list of variables and
  use `echo` to preview the substitution without invoking any commands.

### Command line mode

The client can also be used without the interactive shell, which can be useful if
you want to pipe the output of the client to another application or invoke client
commands using scripts.

```shell
USAGE:
    sui client [SUBCOMMAND]
```

For example, we can use the following command to see the list of
accounts available on the platform:

```shell
$ sui client addresses
```

The result of running this command should resemble the following output:

```shell
Showing 5 results.
0x66af3898e7558b79e115ab61184a958497d1905a
0xae6fb6036570fec1df71599740c132cdf5b45b9d
0x45cda12e3bafe3017b4b3cd62c493e5fbaad7fb0
0xef999dbdb19ccca504eef5432cec69ea8a1d4a1b
0x4489ab46a230c1876578441d68f25bf968e6f2b0
```

But the actual address values will most likely differ
in your case (as will other values, such as object IDs, in the latter
parts of this tutorial). Consequently, **do not copy and paste
the actual command from this tutorial as they are unlikely to work for
you verbatim**. Each time you create a config for the client, addresses
and object IDs will be assigned randomly. Consequently, you cannot rely
on copy-pasting commands that include these values, as they will be different
between different users/configs.

### Active address

Since a Sui CLI client manages multiple disjointed addresses, one might need to specify
which address they want to call a command on.

For convenience, one can choose to set a default, or active address that will be
used for commands that require an address to operate on. A default address is picked
at the start, but this can be changed later.

In order to see what the current active address is, use the command `active-address`

```shell
$ sui client active-address
```

Which will reveal an address resembling:

```shell
0x562f07cf6369e8d22dbf226a5bfedc6300014837
```

Changing the default address is as easy as calling the `switch` command:

```shell
$ sui client switch --address 0x913cf36f370613ed131868ac6f9da2420166062e
```

You will see output like:

```shell
Active address switched to 0x913cf36f370613ed131868ac6f9da2420166062e
```

One can call, for example, the `objects` command with or without an address specified.
When not specified, the active address is used.

```shell
$ sui client objects
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI> 
```
```shell
$ sui client objects --address 0x913cf36f370613ed131868ac6f9da2420166062e
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI> 
```

All commands where `address` is omitted will now use the newly specified active address:
0x913cf36f370613ed131868ac6f9da2420166062e

Note that if one calls a command that uses a gas object not owned by the active address,
the address owned by the gas object is temporarily used for the transaction.

### Paying For transactions with gas objects

All Sui transactions require a gas object for payment, as well as a budget. However, specifying
the gas object can be cumbersome; so in the CLI, one is allowed to omit the gas object and leave
the client to pick an object that meets the specified budget. This gas selection logic is currently
rudimentary as it does not combine/split gas as needed but currently picks the first object it finds
that meets the budget. Note that one can always specify their own gas if they want to manage the gas
themselves.

:warning: A gas object cannot be part of the transaction while also being used to
pay for the transaction. For example, one cannot try to transfer gas object X while paying for the
transaction with gas object X. The gas selection logic checks for this and rejects such cases.

To see how much gas is in an account, use the `gas` command. Note that this command uses the
`active-address`, unless otherwise specified.

```shell
$ sui client gas
```

You will see output like:

```shell
                Object ID                   |  Version   |  Gas Value
------------------------------------------------------------------------
 0x0b8a4620426e526fa42995cf26eb610bfe6bf063 |     0      |   100000
 0x3c0763ccdea4ff5a4557505a62ab5e1daf91f4a2 |     0      |   100000
 0x45a589a9e760d7f75d399327ac0fcba21495c22e |     0      |   100000
 0x4c377a3a9d4b1b9c92189dd12bb1dcd0302a954b |     0      |   100000
 0xf2961464ac6860a05d21b48c020b7e121399965c |     0      |   100000
```

If one does not want to use the active address, the addresses can be specified:

```shell
$ sui client gas --address 0x562f07cf6369e8d22dbf226a5bfedc6300014837
                Object ID                   |  Version   |  Gas Value
------------------------------------------------------------------------
 0xa8ddc2661a19010e5f85cbf6d905ddfbe4dd0320 |     0      |   100000
 0xb2683d0b592e5b002d110989a52943bc9da19158 |     0      |   100000
 0xb41bf45b01c9befce3a0a371e2b98e062691438d |     0      |   100000
 0xba9e10f319182f3bd584edb92c7899cc6d018723 |     0      |   100000
 0xf8bfe77a5b21e7abfa3bc285991f9da4e5cc2d7b |     0      |   100000

```

## Adding accounts to the client

Sui's genesis process will create five accounts by default; if that's
not enough, there are two ways to add accounts to the Sui CLI client if needed.

### Generating a new account

To create a new account, execute the `new-address` command:

```shell
$ sui client new-address
```

The output shows a confirmation after the account has been created:

```
Created new keypair for address : 0xc72cf3adcc4d11c03079cef2c8992aea5268677a
```

### Add existing accounts to `client.yaml` manually

If you have an existing key pair from an old client config, you can copy the account
address manually to the new `client.yaml`'s accounts section, and add the key pair to the keystore file;
you won't be able to mutate objects if the account key is missing from the keystore.

Restart the Sui console after the modification; the new accounts will appear in the client if you query the addresses.

## View objects owned by the address

You can use the `objects` command to view the objects owned by the address.

`objects` command usage:

```shell
sui-client-objects 
Obtain all objects owned by the address

USAGE:
    sui client objects [OPTIONS]

OPTIONS:
        --address <ADDRESS>    Address owning the objects
    -h, --help                 Print help information
        --json                 Return command outputs in json format
```

To view the objects owned by the addresses created in genesis, run the following command (substituting the address with one of the genesis addresses in your client):

```shell
$ sui client objects --address 0x66af3898e7558b79e115ab61184a958497d1905a
```

The result should resemble the following.

```shell
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19 |     0      | uCZNPmDWOksKhCKwEaMtST5T4HbTjcgXGHRXP4qTLC8= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xd1949864f94d87cf25e1fd7b1c8ab4bf685f7801 |     0      | OsTryyECAPW9mnSbWlYWELX+QlRg5er7s/DlkgqhDww= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xddb6119c320f52f3fef9fbc272af305d985b6883 |     0      | gBCDdel7iJZnXpuf4g9dqIPT4XjaAY/4knNcDxbTons= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xe1fe79ac8d900342e617e0986f54ff64e4e323de |     0      | qjsWIzAaomo0eqFwQt99EkARsiC/aw2hPDH8quM6pYg= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 5 results.
```

If you want to view more information about the objects, you can use the `object` command.

Usage of `object` command :

```shell
sui-client-object 
Get object info

USAGE:
    sui client object [OPTIONS] --id <ID>

OPTIONS:
    -h, --help       Print help information
        --id <ID>    Object ID of the object to fetch
        --json       Return command outputs in json format
```

To view the object, use the following command:

```bash
$ sui client object --id 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55
```

This should give you output similar to the following:

```shell
----- Move Object (0x66eaa38c8ea99673a92a076a00101ab9b3a06b55[0]) -----
Owner: Account Address ( 0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0 )
Version: 0
Storage Rebate: 0
Previous Transaction: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
----- Data -----
type: 0x2::coin::Coin<0x2::sui::SUI>
balance: 100000
id: 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55[0]
```

The result shows some basic information about the object, the owner,
version, ID, if the object is immutable and the type of the object.

> **Important:** To gain a deeper view into the object, include the
> `--json` flag in the `sui client` command to see the raw JSON representation
> of the object.

Here is example `json` output:

```json
{
  "data": {
    "dataType": "moveObject",
    "type": "0x2::coin::Coin<0x2::sui::SUI>",
    "has_public_transfer": true,
    "fields": {
      "balance": 100000,
      "id": {
        "id": "0x66eaa38c8ea99673a92a076a00101ab9b3a06b55",
        "version": 0
      }
    }
  },
  "owner": {
    "AddressOwner": "0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0"
  },
  "previousTransaction": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  "storageRebate": 0,
  "reference": {
    "objectId": "0x66eaa38c8ea99673a92a076a00101ab9b3a06b55",
    "version": 0,
    "digest": "j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo="
  }
}
```

## Transferring coins

Coins *are* objects yet have a specific use case that allow for native commands like transfer-coin/merge-coin/split-coin to be used. This is different from non-coin objects that can only be mutated via [Move calls](#calling-move-code).

If you inspect a newly created account, you would expect the account does not own any object. Let us inspect the fresh account we create in the [Generating a new account](#generating-a-new-account) section (`C72CF3ADCC4D11C03079CEF2C8992AEA5268677A`):

```shell
$ sui client objects --address 0xc72cf3adcc4d11c03079cef2c8992aea5268677a
                 Object ID                  |  Version   |                                Digest
------------------------------------------------------------------------------------------------------------------------------
Showing 0 results.
```

To add objects to the account, you can [invoke a Move function](#calling-move-code),
or you can transfer one of the existing coins from the genesis account to the new account using a dedicated Sui client command.
We will explore how to transfer coins using the Sui CLI client in this section.

`transfer-coin` command usage:

```shell
sui-client-transfer-coin 
Transfer coin object

USAGE:
    sui client transfer-coin [OPTIONS] --to <TO> --coin-object-id <COIN_OBJECT_ID> --gas-budget <GAS_BUDGET>

OPTIONS:
        --coin-object-id <COIN_OBJECT_ID>
            Coin to transfer, in 20 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
            object with at least gas_budget value will be selected

        --gas-budget <GAS_BUDGET>
            Gas budget for this transfer

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --to <TO>
            Recipient address
```

To transfer a coin object to a recipient, you will need the recipient's address,
the object ID of the coin that you want to transfer,
and optionally the coin object ID for the transaction fee payment. If a gas
coin is not specified, one that meets the budget is picked. Gas budget sets a
cap for how much gas you want to spend. We are still finalizing our gas metering
mechanisms. For now, just set something large enough.

Here is an example transfer of an object to account `0xf456ebef195e4a231488df56b762ac90695be2dd`:

```shell
$ sui client transfer-coin --to 0xf456ebef195e4a231488df56b762ac90695be2dd --coin-object-id 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 --gas-budget 100
```

With output like:

```
Transfer confirmed after 16896 us
----- Certificate ----
Transaction Hash: mjj1+0Wn+lER1oSD7fwXmoaxzoZW1pmMOqHQJgniy8U=
Transaction Signature: YLbToj+MjgQnaix24ObbE+BdXna6bB9gSSm+YMa/VHsX5g68T9+5vRnGbvDECGoioluUQP0k/zSPvQU5Y/uXCA==@BE/TaOYjyEtJUqF0Db4FEcVT4umrPmp760gFLQIGA1E=
Signed Authorities : [k#f2e5749a5fc33d45c6f546eb9e53fabf4f17681ba6f697080de9514f4e0d6a75, k#3adde8bfae7d338b65e7d13d4ead6b523e5271ca17b2d5eb321412257ee914a4, k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56]
Transaction Kind : Public Transfer Object
Recipient : 0xf456ebef195e4a231488df56b762ac90695be2dd
Object ID : 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55
Version : SequenceNumber(1)
Object Digest : NFDitxwq+bXetYmBxsw9RYEFqq+NWIbRxVoyv3JJXSE=
----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 , Owner: Account Address ( 0xf456ebef195e4a231488df56b762ac90695be2dd )
  - ID: 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19 , Owner: Account Address ( 0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0 )
```

The account will now have one object:

```shell
$ sui client objects --address 0xc72cf3adcc4d11c03079cef2c8992aea5268677a
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     1      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
```

## Creating example NFTs

You may create an [NFT-like object](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/devnet_nft.move#L16) on Sui using the following command:

```shell
$ sui client create-example-nft
```

You will see output resembling:

```shell
Successfully created an ExampleNFT:

----- Move Object (0x524f9fae3ca4554e01354415daf58a05e5bf26ac[1]) -----
Owner: Account Address ( 0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0 )
Version: 1
Storage Rebate: 25
Previous Transaction: 98HbDxEwEUknQiJzyWM8AiYIM479BEKuGwxrZOGtAwk=
----- Data -----
type: 0x2::devnet_nft::DevNetNFT
description: An NFT created by the Sui Command Line Tool
id: 0x524f9fae3ca4554e01354415daf58a05e5bf26ac[1]
name: Example NFT
url: ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty
```

The command will invoke the `mint` function in the `devnet_nft` module, which mints a Sui object with three attributes: name, description, and image URL with [default values](https://github.com/MystenLabs/sui/blob/27dff728a4c9cb65cd5d92a574105df20cb51887/sui/src/wallet_commands.rs#L39) and transfers the object to your address. You can also provide custom values using the following instructions:


`create-example-nft` command usage:

```shell
sui-client-create-example-nft 
Create an example NFT

USAGE:
    sui client create-example-nft [OPTIONS]

OPTIONS:
        --description <DESCRIPTION>    Description of the NFT
        --gas <GAS>                    ID of the gas object for gas payment, in 20 bytes Hex string
                                       If not provided, a gas object with at least gas_budget value
                                       will be selected
        --gas-budget <GAS_BUDGET>      Gas budget for this transfer
    -h, --help                         Print help information
        --json                         Return command outputs in json format
        --name <NAME>                  Name of the NFT
        --url <URL>                    Display url(e.g., an image url) of the NFT

```


## Merging and splitting coin objects

Overtime, the account might receive coins from other accounts and will become unmanageable when
the number of coins grows; contrarily, the account might need to split the coins for payment or
for transfer to another account.

We can use the `merge-coin` command and `split-coin` command to consolidate or split coins, respectively.

### Merge coins

Usage of `merge-coin`:

```shell
sui-client-merge-coin 
Merge two coin objects into one coin

USAGE:
    sui client merge-coin [OPTIONS] --primary-coin <PRIMARY_COIN> --coin-to-merge <COIN_TO_MERGE> --gas-budget <GAS_BUDGET>

OPTIONS:
        --coin-to-merge <COIN_TO_MERGE>
            Coin to be merged, in 20 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
            object with at least gas_budget value will be selected

        --gas-budget <GAS_BUDGET>
            Gas budget for this call

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --primary-coin <PRIMARY_COIN>
            Coin to merge into, in 20 bytes Hex string
```

Here is an example of how to merge coins. To merge coins, you will need at lease three coin objects -
two coin objects for merging, and one for the gas payment.
You also need to specify the maximum gas budget that should be expanded for the coin merge operations.
Let us examine objects owned by address `0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75`
and use the first coin (gas) object as the one to be the result of the merge, the second one to be merged, and the third one to be used as payment:

```shell
$ sui client objects --address 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
```

And its output:

```
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x1e90389f5d70d7fa6ce973155460e1c04deae194 |     0      | BC5O8Bf6Uw8S1LV1y4RCI6+kz1KhZG/aOpeqq9kTAvs= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x351f08f03709cebea85dcd20e24b00fbc1851c92 |     0      | 9aYvavAzY6chYbOUtMtJj0g/5GNc+KBsqptCX5pmQ2Y= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x3c720502f9eabb17a52a999859fbbaeb408b1d14 |     0      | WUPT6P40veMZ/C7GiQpv92I4EH+hvh5BbkBt+7p9yH0= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x7438af4677b9cea2094848f611143346183c11d1 |     0      | 55B56RG/kCeHrN6GXdIq0IvnyYD/hng9J7I7FNRykQ4= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x9d5f2b2564ad2255c24a03556785bddc85381508 |     0      | rmyYjq/UEED0xR0hE3Da8OYgBAu3MYxKQ3v76pGTDek= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 5 results.
```

Then we merge:
```shell
$ sui client merge-coin --primary-coin 0x1e90389f5d70d7fa6ce973155460e1c04deae194 --coin-to-merge 0x351f08f03709cebea85dcd20e24b00fbc1851c92 --gas-budget 1000
```

With results resembling:

```
----- Certificate ----
Transaction Hash: kxxpeggKaMpiWTpSrCNYcu3EDBfNWBJiIPnqae99Znw=
Transaction Signature: /4jxUHC8iZRaHlgbgfOr962BqIRb7AavVJE8GUlY6EMehedF8iVxPf8URe5wFyrxD8IvEclN3Z1qJ4UweYCQAA==@cQeSjZ1xq4QC+7G5/MlAhnZuie6ZrukU/ps2LHmX3D8=
Signed Authorities : [k#3adde8bfae7d338b65e7d13d4ead6b523e5271ca17b2d5eb321412257ee914a4, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac, k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56]
Transaction Kind : Call
Package ID : 0x2
Module : coin
Function : join
Arguments : ["0x1e90389f5d70d7fa6ce973155460e1c04deae194", "0x351f08f03709cebea85dcd20e24b00fbc1851c92"]
Type Arguments : ["0x2::sui::SUI"]
----- Merge Coin Results ----
Updated Coin : Coin { id: 0x1e90389f5d70d7fa6ce973155460e1c04deae194, value: 200000 }
Updated Gas : Coin { id: 0x3c720502f9eabb17a52a999859fbbaeb408b1d14, value: 99444 }
```

### Split coins

Usage of `split-coin`:

```shell
sui-client-split-coin 
Split a coin object into multiple coins

USAGE:
    sui client split-coin [OPTIONS] --coin-id <COIN_ID> --amounts <AMOUNTS>... --gas-budget <GAS_BUDGET>

OPTIONS:
        --amounts <AMOUNTS>...       Amount to split out from the coin
        --coin-id <COIN_ID>          Coin to Split, in 20 bytes Hex string
        --gas <GAS>                  ID of the gas object for gas payment, in 20 bytes Hex string If
                                     not provided, a gas object with at least gas_budget value will
                                     be selected
        --gas-budget <GAS_BUDGET>    Gas budget for this call
    -h, --help                       Print help information
        --json                       Return command outputs in json format
```

For splitting coins, you will need at lease two coins to execute the `split-coin` command,
one coin to split, one for the gas payment.

Let us examine objects owned by address `0x08da15bee6a3f5b01edbbd402654a75421d81397`:

```shell
$ sui client objects --address 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

With output resembling:

```shell
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 |     0      | uNcjv6KP8AXgQHTFmiEPV3tpWZcYHb1HmBR0B2pMsAo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x692c179dc434ceb0eaa51cdd198bb905b5ab27c4 |     0      | /ug6IGGld90PqnmL9qijciCqn25V11nn5/PAsKjxMY0= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x7f7b7c1589aceb073a7c8740b1d47d05e4d89e3c |     0      | N5+qKRenKWqb7Y6WKZuFD+fRDB6pj/OtIyri+FSQ3Q0= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xe42558e82e315c9c81ee5b9f1ac3db819ece5c1d |     0      | toHeih0DeFrqxQhGzVUi9EkVwAZSbLx6hv2gpMgNBbs= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xfa322fee6a7f4c266ad4840e85bf3d87689b6de0 |     0      | DxjnkJTSl0o6HlzeOX5K/If61bbFwvRDydjzd2bq8ho= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 5 results.
```

Here is an example of splitting coins. We are splitting out three new coins from the original coin (first one on the list above),
with values of 1000, 5000 and 3000, respectively; note the `--amounts` argument accepts list of values.
We use the second coin on the list to pay for this transaction.

```shell
$ sui client split-coin --coin-id 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 --amounts 1000 5000 3000 --gas-budget 1000
```

You will see output resembling:

```
----- Certificate ----
Transaction Hash: qpxpv+EySl6tkz7OZ+/h/cpOlC/q1kBepr/qrDHsg7k=
Transaction Signature: BsuWPuG9iBnvc/cQBbpBvDsBnzLXrhxPpoblpZ7ZcTQ78X9AtPO7knOaPjEbLxEJMGpOCPTIWa0eMPpoqT/SDQ==@ZXB4tfniuC6Oir8aVtIR5C00Md/tG3WSZRNN7nDDZLs=
Signed Authorities : [k#3adde8bfae7d338b65e7d13d4ead6b523e5271ca17b2d5eb321412257ee914a4, k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#f2e5749a5fc33d45c6f546eb9e53fabf4f17681ba6f697080de9514f4e0d6a75]
Transaction Kind : Call
Package ID : 0x2
Module : coin
Function : split_vec
Arguments : ["0x4a2853304fd2c243dae7d1ba58260bb7c40724e1", [1000,5000,3000]]
Type Arguments : ["0x2::sui::SUI"]
----- Split Coin Results ----
Updated Coin : Coin { id: 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1, value: 91000 }
New Coins : Coin { id: 0x1da8193ac29f94f8207b0222bd5941b7814c1668, value: 3000 },
            Coin { id: 0x3653bae7851c36e0e5e827b7c1a2978ef78efd7e, value: 5000 },
            Coin { id: 0xd5b694f67410d5b6cd293128cd48953aaa0a3dce, value: 1000 }
Updated Gas : Coin { id: 0x692c179dc434ceb0eaa51cdd198bb905b5ab27c4, value: 99385 }
```

```
$ sui client objects --address 0x08da15bee6a3f5b01edbbd402654a75421d81397
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x1da8193ac29f94f8207b0222bd5941b7814c1668 |     1      | nAMEV3NZ0zscjO10QQUt1drLvhNXTk4MVLAg1FXTQxw= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x3653bae7851c36e0e5e827b7c1a2978ef78efd7e |     1      | blMuVATrI89PRvqA4Kuv6rNkbuAb+bYhmkMocY7pavw= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 |     1      | uhfauig0guMidpxFyCO6FzhzDfucss+eA6xWzAVF3sU= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x692c179dc434ceb0eaa51cdd198bb905b5ab27c4 |     1      | sWTy2PUbt3UFEKx1Km32dEG7cQscSK+eVc3ChaZCkkA= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x7f7b7c1589aceb073a7c8740b1d47d05e4d89e3c |     0      | N5+qKRenKWqb7Y6WKZuFD+fRDB6pj/OtIyri+FSQ3Q0= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xd5b694f67410d5b6cd293128cd48953aaa0a3dce |     1      | 4V0BC6eopxkN6wIOdm2FVgwN3psNbPvLKQ9/zrYtsDM= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xe42558e82e315c9c81ee5b9f1ac3db819ece5c1d |     0      | toHeih0DeFrqxQhGzVUi9EkVwAZSbLx6hv2gpMgNBbs= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xfa322fee6a7f4c266ad4840e85bf3d87689b6de0 |     0      | DxjnkJTSl0o6HlzeOX5K/If61bbFwvRDydjzd2bq8ho= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 8 results.
```

From the result, we can see three new coins were created in the transaction.

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Sui CLI. Please see our
[Move developer documentation](move.md#first-look-at-move-source-code)
for the first look at Move source code and a description of the
following function we will be calling in this tutorial:

```rust
public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
    coin::transfer(c, Address::new(recipient))
}
```

Please note that there is no real need to use a Move call to transfer
coins as this can be accomplished with a built-in Sui client
[command](#transferring-coins) - we chose this example due to its
simplicity.

Let us examine objects owned by address `0x48ff0a932b12976caec91d521265b009ad5b2225`:

```shell
$ sui client objects --address 0x48ff0a932b12976caec91d521265b009ad5b2225
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x471c8e241d0473c34753461529b70f9c4ed3151b |     0      | MCQIALghS9kQUWMclChmsd6jCuLiUxNjEn9VRV+AhSA= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     0      | VIbuA4fcsitOUmJLQ+FugZWIn7bg6LnVO8eTIAUDzkg= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 5 results.
```

Now that we know which objects are owned by that address,
we can transfer one of them to another address, say the fresh one
we created in the [Generating a new account](#generating-a-new-account) section
(`0xc72cf3adcc4d11c03079cef2c8992aea5268677a`). We can try any object,
but for the sake of this exercise, let's choose the last one on the
list.

We will perform the transfer by calling the `transfer` function from
the sui module using the following Sui client command:

```shell
$ sui client call --function transfer --module sui --package 0x2 --args 0x471c8e241d0473c34753461529b70f9c4ed3151b 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 --gas-budget 1000
```

This is a pretty complicated command so let's explain all of its
parameters one-by-one:

* `--function` - name of the function to be called
* `--module` - name of the module containing the function
* `--package` - ID of the package object where the module containing
  the function is located. (Remember
  that the ID of the genesis Sui package containing the GAS module is
  defined in its manifest file, and is equal to `0x2`.)
* `--args` - a list of function arguments formatted as
  [SuiJSON](sui-json.md) values (hence the preceding `0x` in address
  and object ID):
  * ID of the gas object representing the `c` parameter of the `transfer`
    function
  * address of the new gas object owner
* `--gas` - an optional object containing gas used to pay for this
  function call
* `--gas-budget` - a decimal value expressing how much gas we are
  willing to pay for the `transfer` call to be completed to avoid
  accidental drain of all gas in the gas pay)

Note the third argument to the `transfer` function representing
`TxContext` does not have to be specified explicitly - it
is a required argument for all functions callable from Sui and is
auto-injected by the platform at the point of a function call.

The output of the call command is a bit verbose, but the important
information that should be printed at the end indicates objects
changes as a result of the function call:

```shell
----- Certificate ----
Transaction Hash: KT7sEHzxavRFkLijfKGDqj6kM5bVl1QA1IawJPV2+Go=
Transaction Signature: GIUaa8yAPgy/eSVypVz+fmbjC2mL5kHuYNodUyNcIUMvlUN5XxyPYdL8C25vvH6rYt/ZUDY2ntZU1NHUp4yPCg==@iocJzkLCMJMh1VGZ6sUsw0okqoDP71ed9a4Vf2vWlx4=
Signed Authorities : [k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac, k#3adde8bfae7d338b65e7d13d4ead6b523e5271ca17b2d5eb321412257ee914a4]
Transaction Kind : Call
Package ID : 0x2
Module : sui
Function : transfer
Arguments : ["0x471c8e241d0473c34753461529b70f9c4ed3151b", "0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75"]
Type Arguments : []
----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0x471c8e241d0473c34753461529b70f9c4ed3151b , Owner: Account Address ( 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 )
  - ID: 0x53b50e3020a01e1fd6acf832a871feee240183f0 , Owner: Account Address ( 0x48ff0a932b12976caec91d521265b009ad5b2225 )
```

This output indicates the gas object
was updated to collect gas payment for the function call, and the
transferred object was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) by querying objects that are now owned by
the sender:

```shell
$ sui client objects --address 0x48ff0a932b12976caec91d521265b009ad5b2225
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     1      | st6KVE+nTPsQgtEtxSbgJZCzSSuSB2ZsJAMbXFNLw/k= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 4 results.
```

We can now see this address no longer owns the transferred object.
And if we inspect this object, we can see it has the new
owner, different from the original one:

```shell
$ sui client object --id 0x471c8e241d0473c34753461529b70f9c4ed3151b
```

Resulting in:

```
----- Move Object (0x471c8e241d0473c34753461529b70f9c4ed3151b[1]) -----
Owner: Account Address ( 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 )
Version: 1
Storage Rebate: 15
Previous Transaction: KT7sEHzxavRFkLijfKGDqj6kM5bVl1QA1IawJPV2+Go=
----- Data -----
type: 0x2::coin::Coin<0x2::sui::SUI>
balance: 100000
id: 0x471c8e241d0473c34753461529b70f9c4ed3151b[1]
```

## Publish packages

In order for user-written code to be available in Sui, it must be
*published* to Sui's [distributed ledger](../learn/how-sui-works.md#architecture).
Please see the [Move developer documentation](move.md) for a
description on how to [write a simple Move code package](move.md#writing-a-package),
which we can publish using Sui client's `publish` command.

The publish command
requires us to specify a directory where the user-defined package lives.
It's the path to the `my_move_package` as per the
[package creation description](move.md#writing-a-package), a gas
object that will be used to pay for publishing the package (we use the
same gas object we used to pay for the function call in the
[Calling Move code](#calling-move-code)) section, and gas budget to put
an upper limit we use 1000 as our gas budget.

Let us use the same address for publishing that we used for calling Move code in the previous [section](#calling-move-code) (`0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75`) which now has 4 objects left:

```shell
$ sui client objects --address 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
```

Outputting:

```
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type               
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     1      | st6KVE+nTPsQgtEtxSbgJZCzSSuSB2ZsJAMbXFNLw/k= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>     
Showing 4 results.
```

The whole command to publish a package for address
`0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75` resembles the following (assuming
that the location of the package's sources is in the `PATH_TO_PACKAGE`
environment variable):

```shell
$ sui client publish --path $PATH_TO_PACKAGE/my_move_package --gas-budget 30000
```

The result of running this command should look as follows:

```shell
----- Certificate ----
Transaction Hash: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
Transaction Signature: 7Lqy/KQW86Tq81cUxLMW07AQw1S+D4QLFC9/jMNKrau81eABHpxG2lgaVaAh0c+d5ldYhp75SmpY0pxq0FSLBA==@BE/TaOYjyEtJUqF0Db4FEcVT4umrPmp760gFLQIGA1E=
Signed Authorities : [k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#f2e5749a5fc33d45c6f546eb9e53fabf4f17681ba6f697080de9514f4e0d6a75, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac]
Transaction Kind : Publish
----- Publish Results ----
The newly published package object ID: 0xdbcee02bd4eb326122ced0a8540f15a057d82850

List of objects created by running module initializers:
----- Move Object (0x4ac2df49c3698baaef11ae23b3d8417d7e5ed65f[1]) -----
Owner: Account Address ( 0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0 )
Version: 1
Storage Rebate: 12
Previous Transaction: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
----- Data -----
type: 0xdbcee02bd4eb326122ced0a8540f15a057d82850::m1::Forge
id: 0x4ac2df49c3698baaef11ae23b3d8417d7e5ed65f[1]
swords_created: 0

Updated Gas : Coin { id: 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19, value: 96929 }
```

Please note that running this command resulted in creating an object representing the published package.
From now on, we can use the package object ID (`0xdbcee02bd4eb326122ced0a8540f15a057d82850`) in the Sui client's call
command just like we used `0x2` for built-in packages in the
[Calling Move code](#calling-move-code) section.

Another object created as a result of package publishing is a
user-defined object (of type `Forge`) crated inside initializer
function of the (only) module included in the published package - see
the part of Move developer documentation concerning [module
initializers](move.md#module-initializers) for more details on module
initializers.

Finally, we see that the gas object that was used to pay for
publishing was updated as well.

## Customize genesis

The genesis process can be customized by providing a genesis configuration
file using the `--config` flag.

```shell
$ sui genesis --config <Path to genesis config file>
```

Example `genesis.yaml`:

```yaml
---
validator_genesis_info: ~
committee_size: 4
accounts:
  - gas_objects:
      - object_id: "0xdbac75c4e5a5064875cb8566a533547957092f93"
        gas_value: 100000
    gas_object_ranges: []
move_packages: ["<Paths to custom move packages>"]
sui_framework_lib_path: ~
move_framework_lib_path: ~

```
