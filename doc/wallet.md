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
can be used subsequently to start the network. A `wallet.conf` will
also be generated to be used by the `wallet` binary to manage the
newly created accounts.


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

    addresses      Obtain the Account Addresses managed by the wallet
    call           Call Move
    help           Prints this message or the help of the given subcommand(s)
    new-address    Generate new address and keypair
    object         Get obj info
    objects        Obtain all objects owned by the account address
    publish        Publish Move modules
    sync           Synchronize client state with authorities
    transfer       Transfer funds

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
0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf
ce3c1f3f3cbb5abf7cb492c31a162b58089d03a2e6057b88fd8228435c9d44e7
d346982dd3a61084c6f7f5af0f1b559cdf2921a3e76f403e85925b3dcf1d991d
dc3e8f72f84422ce3b332756520d7730e7a44b6720b0cd91eaf21bf65d56de3e
```

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
./wallet --no-shell objects --address 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
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
--sender 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
```

This a pretty complicated command so let's explain all its parameters
one-by-one:

- `--function` - name of the function to be called
- `--module` - name of the module containing the function
- `--package` - ID of the package object where the module containing
  the function is located (please
  [remember](#a-quick-look-at-the-gas-module) that the ID of the
  genesis FastX package containing the GAS module is defined in its
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
./wallet --no-shell objects --address 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
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
./wallet --no-shell objects --address 5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf
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
  "sui_framework_lib_path": "<Paths to sui framework lib>",
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

