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
gas objects, which are Sui [objects](objects.md) used to pay for Sui
[transactions](transactions.md#transaction-metadata),
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
0DC70EA8CA82FE6966900C697B73A235B8E2346F
3111A757BE55F195201FD9140DCE55EAEB719D35
4E523C1FAFECE13628161C78521E60AA8B602324
D9573E0B7F73B15C4416DCBD9911CC1A9456CF21
FE574F043D282AAF10B9AE8AB337C74BA8B428C3
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
public fun transfer(c: Coin::Coin<GAS>, recipient: address, _ctx: &mut TxContext) {
    Coin::transfer(c, Address::new(recipient))
}
```

Throughout the Move call example we will be using the non-interactive
shell (all commands will be issued within the shell's prompt:
`sui>-$`) that can be started as follows:

``` shell
./wallet
   _____       _    _       __      ____     __
  / ___/__  __(_)  | |     / /___ _/ / /__  / /_
  \__ \/ / / / /   | | /| / / __ `/ / / _ \/ __/
 ___/ / /_/ / /    | |/ |/ / /_/ / / /  __/ /_
/____/\__,_/_/     |__/|__/\__,_/_/_/\___/\__/
--- Suisui 0.1.0 ---
Config path : "./wallet.conf"
Client state DB folder path : "./client_db"
Managed addresses : 5

Welcome to the Sui interactive shell.

sui>-$
```

Let us proceed with listing gas objects owned by the
first address on the list starting with `0DC7`. 


``` shell
sui>-$ objects --address 0DC70EA8CA82FE6966900C697B73A235B8E2346F
```

When looking at the output, let's focus on the first column (ignore
the opening parenthesis) which lists object IDs owned by this address
the rest of the input is replaced with `...` below):

``` shell
(60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...
(B216DCFE027479D0BE9D85A5CD7184E9673452D8 ...
(B56269D5C471367BEDEDDFCBE8A9D928E7C1F170 ...
(D9DBDEDB501C63996E2662DDD23A76A642E8160B ...
(F18F5B785D5766CD85BC2247F8C73F07BFF901BB ...
```

Now that we know which objects are owned by the address starting with
`0DC7`, we can transfer one of them to another address, say one
starting with `3111`. We can try any object, but for the sake of this
exercise, let's choose the last one on the list, that is one whose ID
is starting with `F18F`.

We will perform the transfer by calling the `transfer` function from
the GAS module using the following Sui Wallet command:

``` shell
sui>-$ call --function transfer --module GAS --package 0x2 --args "0xF18F5B785D5766CD85BC2247F8C73F07BFF901BB" "0x3111A757BE55F195201FD9140DCE55EAEB719D35" --gas 60DADCE6E5081C3EFCA162694D7EFD8D99D46636 --gas-budget 1000
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
- `args` - a list of function arguments:
  - ID of the gas object representing the `c` parameter of the `transfer` function
  - address of the new gas object owner
- `--gas` - an object containing gas that will be used to pay for this
  function call that is owned by the address initiating the `transfer`
  function call (i.e., address starting with `0DC7`) - we chose gas
  object whose ID starts with `60DA` but we could have any object
  owned by this address as at this point the only objects in Sui are
  gas objects
- `--gas-budget` - a decimal value expressing how much gas we are
  willing to pay for the `transfer` call to be completed (the gas
  object may contain a lot more gas than 1000 units and we may want to
  prevent it being drained accidentally beyond what we are intended to
  pay)
  
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
60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...
F18F5B785D5766CD85BC2247F8C73F07BFF901BB ...
```

This output indicates that the gas object whose ID starts with `60DA`
was updated to collect gas payment for the function call, and the
object whose ID starts with `F18F` was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) but querying objects that are now owned by
the sender (abbreviated output):

``` shell
sui>-$ objects --address 0DC70EA8CA82FE6966900C697B73A235B8E2346F
Showing 4 results.
(60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...
(B216DCFE027479D0BE9D85A5CD7184E9673452D8 ...
(B56269D5C471367BEDEDDFCBE8A9D928E7C1F170 ...
(D9DBDEDB501C63996E2662DDD23A76A642E8160B ...
```

We can now see that this address no longer owns the object whose ID
starts with `F18F`. On the other hand, if we inspect this object, we can see that it has the new owner:

``` shell
sui>-$ object --id F18F5B785D5766CD85BC2247F8C73F07BFF901BB
Owner: SingleOwner(k#3111a757be55f195201fd9140dce55eaeb719d35)
Version: 1
ID: F18F5B785D5766CD85BC2247F8C73F07BFF901BB
Readonly: false
Type: 0x2::Coin::Coin<0x2::GAS::GAS>
```

## Package Publishing

In order for user-written code to be available in Sui, it must be
_published_ to to Sui's [distributed
ledger](SUMMARY.md#architecture). Please see Move developer
[documentation](move.md) for a
[description](move.md#writing-a-package) on how to write a simple Move
code package, which we can publish using Sui wallet's publish command.

In order to show how to publish user-defined Move package, let us
continue where we left off in the previous
[section](#calling-move-code) describing how to call an existing Move
function. The publish command requires us to specify a directory where
user-defined package lives (it's a path to the `my_move_package` as
per package creation [description](move.md#writing-a-package)), a gas
object that will be used to pay for publishing the package (we use the
same gas object we used to pay for the function call in the previous
[section](#calling-move-code)), and gas budget to put an upper limit
(we use 1000 as our gas budget. The whole command looks as follows:

``` shell
sui>-$ publish --path /Users/adamwelc/my_move_package --gas 60DADCE6E5081C3EFCA162694D7EFD8D99D46636 1000
```

The (abbreviated) result of running this command should show that one
object (package object) was created and one object (gas object) was
modified:

``` shell
Created Objects:
DF12826C99CE99E9028D72A7B2CE78CFDAE15B54 ...
Mutated Objects:
60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...

```

From now on, we can use package object id in the Sui wallet's call
command just like we used 0x2 for built-in packages in the previous
[section](#calling-move-code).

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

