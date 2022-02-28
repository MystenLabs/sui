# Wallet Quick Start

Welcome to the Sui tutorial focusing on the toy Sui wallet developed
to facilitate local experimentation with Sui features using a
command-line interface. In this document we will describe how to setup
Sui wallet and how to execute wallet commands through its command-line
interface (Wallet CLI).


## Setup

### Build the binaries

```shell
cargo build --release
cd target/release
```

This will create `sui` and `wallet` binaries in `target/release`
directory.

### Genesis

```shell
./sui genesis
```

The genesis command creates 4 authorities, 5 user accounts each with 5
gas objects, which are Sui objects used to pay for Sui
[transactions](https://github.com/MystenLabs/fastnft/blob/main/doc/transactions.md#transaction-metadata),
such other object transfers or smart contract (Move) calls. These
numbers represent a sample configuration and have been chosen somewhat
arbitrarily, but the process of generating the genesis state can be
customized with additional accounts, objects, code, etc. as described
[here](#genesis-customization).


The network configuration is stored in `network.conf` and
can be used subsequently to start the network. A `wallet.conf` is
also created to be used by the Sui wallet to manage the
newly created accounts.

### View created accounts
The genesis process creates a configuration file `wallet.conf` for the Sui wallet, 
the config file contains information of the accounts and the Sui network, 
Sui wallet uses the network information to communicate with the Sui network authorities 
and create transactions using the key pairs residing in the config file.

Below is an example of `wallet.conf` showing the accounts and key pairs in the wallet configuration. (some values are omitted)
```json
{
  "accounts": [
    {
      "address": "a4c0a493ce9879ea06bac93461810cf947823bb3",
      "key_pair": "MTpXG/yJq0OLOknghzYRCS6D/Rz+97gpR7hZhUCUNT5pMCy49v7hZkCSHm38e+cp+sdxLgTrSAuCbDxqkF1MTg=="
    },
    ...
  ],
  "authorities": [
    {
      "name": "d72e7bd8f435fa56af47c8d0a0b8738c48f446762d620863ac328605325692f7",
      "host": "127.0.0.1",
      "base_port": 10000
    },
    ...
  ],
  "send_timeout": {
    "secs": 4,
    "nanos": 0
  },
  "recv_timeout": {
    "secs": 4,
    "nanos": 0
  },
  "buffer_size": 65507,
  "db_folder_path": "./client_db"
}
```
The `accounts` variable contains all the account's address and key pairs, this will be used by the wallet to sign transactions.
`authorities` contains Sui network authorities' name, host and port information, it is used to establish connections to the Sui network.

`send_timeout`, `recv_timeout` and `buffer_size` are the network parameters.
`db_folder_path` is the path to the account's client state database, which will be storing 
all the transaction data, certificates and object data belonging to the account.

#### Key management
As you might have noticed, the key pairs are stored as plain text in `wallet.conf`, 
which is not secure and shouldn't be used in production environment, we have plans to 
implement more secure key management and support hardware signing in future release.

:warning: **Do not use in production**: Keys are stored in plain text!

### Starting the network

Run the following command to start the local Sui network:

```shell
./sui start 
```

or

```shell
./sui start --config [config file path]
```

The network config file path is defaulted to `./network.conf` if not
specified.


### Running interactive wallet

To start the interactive wallet:

```shell
./wallet
```

or

```shell
./wallet --config [config file path]
```

The wallet config file path is defaulted to `./wallet.conf` if not specified.  

The following commands are supported by the interactive wallet:

    addresses      Obtain the Addresses managed by the wallet
    call           Call Move function
    gas            Obtain all gas objects owned by the address
    help           Prints this message or the help of the given subcommand(s)
    new-address    Generate new address and keypair
    object         Get obj info
    objects        Obtain all objects owned by the address
    publish        Publish Move modules
    sync           Synchronize client state with authorities
    transfer       Transfer an object

Use `help <command>` to see more information on each command.

The wallet can also be used without the interactive shell

```shell
USAGE:
    wallet --no-shell [SUBCOMMAND]
```

For example, we can use the following command to see the list of
accounts available on the platform:

``` shell
./wallet --no-shell addresses
```

The result of running this command should look similarly to the
following one, but the actual address values will most likely differ
in your case (as will other values, such as object IDs, in the later
parts of this tutorial). Consequently, **please do not copy and paste
the actual command from this tutorial as they are unlikely to work for
you verbatim**.

```shell
Showing 5 results.
054FDE3B99A88D8A176CF2E795A18EDC19B32D21
19A6D4720C1C7D16583A9FB3FB537EF31E169DE6
20007C278237AA4BED5FF6094AA05A7FEC90F932
594E3CD1445281FF1BB7D1892534F0235FF810BA
A4C0A493CE9879EA06BAC93461810CF947823BB3
```

## Adding accounts to the wallet
Sui's genesis process will create five accounts by default, if that's not enough, there are two ways to add accounts to the Sui wallet if needed.
#### 1. use `new-address` command to generate new account
To create a new account, execute the `new-address` command in Sui interactive console:
``` shell
sui>-$ new-address
```
The console should return a confirmation after the account has been created.
```
Created new keypair for address : 3F8962C87474F8FB8BFB99151D5F83E677062078
```
  
#### 2. Add existing accounts to `wallet.conf` manually.
If you have existing key pair from an old wallet config, you can copy the account data manually to the new `wallet.conf`'s accounts section.

The account data looks like this: 
```json
    {
      "address": "a4c0a493ce9879ea06bac93461810cf947823bb3",
      "key_pair": "MTpXG/yJq0OLOknghzYRCS6D/Rz+97gpR7hZhUCUNT5pMCy49v7hZkCSHm38e+cp+sdxLgTrSAuCbDxqkF1MTg=="
    }
```
Restart the Sui wallet after the modification, the new accounts will appear in the wallet if you query the addresses.

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Wallet CLI. Please see Move
developer [documentation](move.md#first-look-at-move-source-code) for
the first look at Move source code and a description of the following
function we will be calling in this tutorial:

```rust
public fun transfer(c: Coin::Coin<GAS>, recipient: vector<u8>, _ctx: &mut TxContext) {
    Coin::transfer(c, Address::new(recipient))
}
```

At the end of the previous [section](#running-interactive-wallet) we
learned how to find out the user addresses available as part of Sui
genesis setup. Let us proceed with listing gas objects owned by the
first address on the list starting with `0523`, which can be
accomplished using the following command:


``` shell
./wallet --no-shell objects --address 054FDE3B99A88D8A176CF2E795A18EDC19B32D21
```

When looking at the output, let's focus on the first column which
lists object IDs owned by this address (the rest of the input is
replaced with `...` below):

``` shell
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1: ...
363D5BCAC9D5855122202B6B832B321D4256F22E: ...
7022F48406251C0D5AE4EBEBB4C7150F3D34E195: ...
771101CE95E5A774D94E172CD54178C124327EB6: ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F: ...
```

Now that we know which objects are owned by the address starting with
`0523`, we can transfer one of them to another address, say one
starting with `5986`. We can try any object, but for the sake of this
exercise, let's choose the last one on the list, that is one whose ID
is starting with `B800`.

We will perform the transfer by calling the `transfer` function from
the GAS module using the following Sui Wallet command:

``` shell
./wallet --no-shell call \
--function transfer \
--module GAS \
--package 0x2 \
--object-args B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F \
--pure-args x\"5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf\" \
--gas 1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1 \
--gas-budget 1000 \
```

This is a pretty complicated command so let's explain all its parameters
one-by-one:

- `--function` - name of the function to be called
- `--module` - name of the module containing the function
- `--package` - ID of the package object where the module containing
  the function is located (please
  [remember](#a-quick-look-at-the-gas-module) that the ID of the
  genesis Sui package containing the GAS module is defined in its
  manifest file, and is equal to 0x2)
- `object-args` - a list of arguments of Sui object type (in this case
  there is only one representing the `c` parameter of the `transfer`
  function)
- `pure-args` - a list of arguments of Sui primitive types or vectors
  of such types (in this case there is only one representing the
  `recipient` parameter of the `transfer` function)
- `--gas` - an object containing gas that will be used to pay for this
  function call that is owned by the address initiating the `transfer`
  function call (i.e., address starting with `0523`) - we chose gas
  object whose ID starts with `1FD8` but we could have any object
  owned by this address as at this point the only objects in Sui are
  gas objects
- `--gas-budget` - a decimal value expressing how much gas we are
  willing to pay for the `transfer` call to be completed (the gas
  object may contain a lot more gas than 1000 units and we may want to
  prevent it being drained accidentally beyond what we are intended to
  pay)
- `--sender` - the address of the account initiating the function
  call, which also needs to own the object to be transferred
  
Please note that the third argument to the `transfer` function
representing `TxContext` does not have to be specified explicitly - it
is a required argument for all functions callable from Sui and is
auto-injected by the platform at the point of a function call.

The output of the call command is a bit verbose, but the important
information that should be printed at the end indicates objects
changes as a result of the function call (we again abbreviate the
output to only include the first column of the object description
containing its ID):

``` shell
...
Mutated Objects:
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1 ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F ...
```

This output indicates that the gas object whose ID starts with `1FD8`
was updated to collect gas payment for the function call, and the
object whose ID starts with `B800` was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) but querying objects that are now owned by
the sender (abbreviated output):

``` shell
./wallet --no-shell objects --address 054FDE3B99A88D8A176CF2E795A18EDC19B32D21
Showing 4 results.
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1: ...
363D5BCAC9D5855122202B6B832B321D4256F22E: ...
7022F48406251C0D5AE4EBEBB4C7150F3D34E195: ...
771101CE95E5A774D94E172CD54178C124327EB6: ...
```

We can now see that this address no longer owns the object whose IS
starts with `B800`. On the other hand, the recipient now owns 6
objects including the transferred one (in the fourth position):

``` shell
./wallet --no-shell objects --address 054FDE3B99A88D8A176CF2E795A18EDC19B32D21
Showing 6 results.
348B607E5C8B80524D6BF8275FB7F35267A7814E: ...
5852529FE26D138D7B6B9281ADBF29645D93543A: ...
87128A733E6F8AE432C2B928A432309FD1E70363: ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F: ...
C80707F7D1C8CBAC58BFD9A1EAD18199F0ECE931: ...
DC5530627AFBFFBB1F52B81F273A7B666B31CB85: ...
```

## Package Publishing

TBD


## Genesis customization

The genesis process can be customized by providing a genesis config
file.

```shell
./sui genesis --config genesis.conf
```
Example `genesis.conf`
```json
{
  "authorities": [
    {
      "key_pair": "xWhgxF5fagohi2V9jzUToxnhJbTwbtV2qX4dbMGXR7lORTBuDBe+ppFDnnHz8L/BcYHWO76EuQzUYe5pnpLsFQ==",
      "host": "127.0.0.1",
      "port": 10000,
      "db_path": "./authorities_db/4e45306e0c17bea691439e71f3f0bfc17181d63bbe84b90cd461ee699e92ec15",
      "stake": 1
    }
  ],
  "accounts": [
    {
      "address": "bd654f352c895d9ec14c491d3f2b4e1f98fb07323383bebe9f95ab625bff2fa0",
      "gas_objects": [
        {
          "object_id": "5c68ac7ba66ef69fdea0651a21b531a37bf342b7",
          "gas_value": 1000
        }
      ]
    }
  ],
  "move_packages": ["<Paths to custom move packages>"],
  "sui_framework_lib_path": "<Paths to Sui framework lib>",
  "move_framework_lib_path": "<Paths to move framework lib>"
}
```

All attributes in genesis.conf are optional, default value will be use
if the attributes are not provided.  For example, the config shown
below will create a network of 4 authorities, and pre-populate 2 gas
objects for 4 accounts.

```json
{
  "authorities": [
    {},{},{},{}
  ],
  "accounts": [
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] }
  ]
}
```

