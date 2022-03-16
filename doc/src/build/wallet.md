---
title: Wallet Quick Start
---

Welcome to the Sui tutorial on the sample Sui wallet developed
to facilitate local experimentation with Sui features using a
command line interface. In this document, we describe how to set up
Sui wallet and execute wallet commands through its command line
interface, *Wallet CLI*.


## Setup

Follow the instructions to [install Sui binaries](install.md).

## Genesis
```shell
sui genesis
```
NOTE: For logs, set `RUST_LOG=debug` before invoking `sui genesis`.

The `genesis` command creates four authorities and five user accounts
each with five gas objects. These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
such other object transfers or smart contract (Move) calls. These
numbers represent a sample configuration and have been chosen somewhat
arbitrarily; the process of generating the genesis state can be
customized with additional accounts, objects, code, etc. as described
in [Genesis customization](#customize-genesis).

The network configuration is stored in `network.conf` and
can be used subsequently to start the network. `wallet.conf` and `wallet.key` are
also created to be used by the Sui wallet to manage the
newly created accounts.

## Wallet configuration
The genesis process creates a configuration file `wallet.conf`, and a keystore file `wallet.key` for the
Sui wallet.  The config file contains information of the accounts and
the Sui network gateway. The keystore file contains all the public-private key pair of the created accounts.
Sui wallet uses the network information in `wallet.conf` to communicate
with the Sui network authorities  and create transactions using the key
pairs residing in the keystore file.

Here is an example of `wallet.conf` showing the accounts and key pairs
in the wallet configuration (with some values omitted):

```json
{
  "accounts": [
    "48cf013a76d583c027720f7f9852deac7c84b923",
    ...
  ],
  "keystore": {
    "File": "./wallet.key"
  },
  "gateway": {
    "embedded": {
      "authorities": [
        {
          "name": "5f9701f4bd2cd7c2f1f23ac6d05515407879f0acf2611517ff188e59c5f61743",
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
  }
}
```
The `accounts` variable contains the account's address the wallet manages.
`gateway` contains the information of the Sui network that the wallet will be connecting to,
currently only `Embedded` gateway type is supported.

The `authorities` variable is part of the embedded gateway configuration, it contains Sui network
authorities' name, host and port information. It is used to establish connections to the Sui network.

Note `send_timeout`, `recv_timeout` and `buffer_size` are the network
parameters and `db_folder_path` is the path to the account's client state
database. This database stores all the transaction data, certificates
and object data belonging to the account.

### Sui Network Gateway
The Sui network gateway is an abstraction layer that acts as the entry point to the Sui network.
Different gateway implementation can be use by the application layer base on their use cases.

#### Embedded Gateway
As the name suggests, embedded gateway embeds the gateway logic into the application;
all data will be stored locally and the application will make direct
connection to the authorities.

### Key management
The key pairs are stored in `wallet.key`. However, this is not secure
and shouldn't be used in a production environment. We have plans to
implement more secure key management and support hardware signing in a future release.

:warning: **Do not use in production**: Keys are stored in file!

## Starting the network
Run the following command to start the local Sui network:

```shell
sui start
```

or

```shell
sui start --config [config file path]
```
NOTE: For logs, set `RUST_LOG=debug` before invoking `sui start`.

The network config file path defaults to `./network.conf` if not
specified.

## Using the wallet
The following commands are supported by the wallet:

    `addresses`      Obtain the Addresses managed by the wallet
    `call`           Call Move function
    `gas`            Obtain all gas objects owned by the address
    `help`           Prints this message or the help of the given subcommand(s)
    `merge-coin`     Merge two coin objects into one coin
    `new-address`    Generate new address and key-pair
    `object`         Get object info
    `objects`        Obtain all objects owned by the address
    `publish`        Publish Move modules
    `split-coin`     Split a coin object into multiple coins
    `sync`           Synchronize client state with authorities
    `transfer`       Transfer an object
Use `help <command>` to see more information on each command.

The wallet can be started in two modes: interactive shell or command line interface.

### Interactive shell

To start the interactive shell:

```shell
wallet
```

or

```shell
wallet --config [config file path]
```

The wallet config file path defaults to `./wallet.conf` if not
specified.

The Sui interactive wallet supports the following shell functionality:
* Command History
  The `history` command can be used to print the interactive shell's command history; 
  you can also use Up, Down or Ctrl-P, Ctrl-N to navigate previous or next matches from history. 
  History search is also supported using Ctrl-R.
* Tab completion
  Tab completion is supported for all commands using Tab and Ctrl-I keys.
* Environment variable substitution
  The wallet shell will substitute inputs prefixed with `$` with environment variables, 
  you can use the `env` command to print out the entire list of variables and 
  use `echo` to preview the substitution without invoking any commands.  

### Command line mode

The wallet can also be used without the interactive shell, which can be useful if 
you want to pipe the output of the wallet to another application or invoke wallet 
commands using scripts.

```shell
USAGE:
    wallet --no-shell [SUBCOMMAND]
```

For example, we can use the following command to see the list of
accounts available on the platform:

```shell
wallet --no-shell addresses
```

The result of running this command should resemble the following output:

```shell
Showing 5 results.
0999FD9EEE3AD557112182E7CB5747A253132000
23F1D33B058CB784C0740A0139ED81AC71A11CE3
8F89E566BFB2F68DE0DB8E64F8335D957792A7E8
E7EFB976F10753666C821400FD9554B766363317
FF4480C3BB1E1B15CF245667B8448D930D2A05BB
```

But the actual address values will most likely differ
in your case (as will other values, such as object IDs, in the later
parts of this tutorial). Consequently, **do not copy and paste
the actual command from this tutorial as they are unlikely to work for
you verbatim**. Each time you create a config for the wallet, addresses
and object IDs will be assigned randomly. Consequently, you cannot rely
on copy-pasting commands that include these values, as they will be different
between different users/configs.

## Adding accounts to the wallet

Sui's genesis process will create five accounts by default; if that's
not enough, there are two ways to add accounts to the Sui wallet if needed.

### Generating a new account

To create a new account, execute the `new-address` command:

```shell
wallet --no-shell new-address
```

The output shows a confirmation after the account has been created:

```
Created new keypair for address : F456EBEF195E4A231488DF56B762AC90695BE2DD
```

### Add existing accounts to `wallet.conf` manually.

If you have an existing key pair from an old wallet config, you can copy the account
address manually to the new `wallet.conf`'s accounts section, and add the key pair to the keystore file;
you won't be able to mutate objects if the account key is missing from the keystore.

Restart the Sui wallet after the modification; the new accounts will appear in the wallet if you query the addresses.

## View objects owned by the account
You can use the `objects` command to view the objects owned by the address.

`objects` command usage :

```shell
USAGE:
    objects [FLAGS] --address <address>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --address <address>    Address owning the objects
```
To view the objects owned by the accounts created in genesis, run the following command (substitute the address with one of the genesis addresses in your wallet):
```shell
wallet --no-shell objects --address 0999FD9EEE3AD557112182E7CB5747A253132000
```
The result should resemble the following, which shows the object in the format of (`object_id`, `sequence_number`, `object_hash`).
```shell
Showing 5 results.
(531AE72F84014918704DF57DA990D08EFCA8BF02, SequenceNumber(0), o#fbded551c71121a42e079b5fd179da42c718220a11c9b24d4529e8421266a311)
(587454732C89143D5AD10D1494FBB4CFA2EC56F0, SequenceNumber(0), o#ac8572b1113a09a21812f2a0492aa5285c6550582cc7add757b4980ae4f03a35)
(659EE95880712843537E7553DFF66D98E0CC5ABD, SequenceNumber(0), o#680c60440a4d624f780e013c99b75e906db398b3de85f92183c7d26c7bb378c2)
(9495C4EEEB6F935A2AA19D9BA5B3D1D47A30F32E, SequenceNumber(0), o#5236389cfe8ae26f4d0f62bfa8b8e579bc31ce85446a86aafcc8ac20aa04c3e7)
(C7CC5FA26A039CFA03B32FA56414DFCE19BA318C, SequenceNumber(0), o#6413c14ed7ce43bd3b354c431260725773f838c7a193caae85bac97e10f0d38e)
```
If you want to view more information about the objects, you can use the `object` command.

Usage of `object` command :
```shell
USAGE:
    object [FLAGS] --id <id>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --id <id>    Object ID of the object to fetch
```
To view the object, use the following command:
```bash
wallet --no-shell object --id C7CC5FA26A039CFA03B32FA56414DFCE19BA318C
```
This should give you output similar to the following:
```shell
Owner: AddressOwner(k#0999fd9eee3ad557112182e7cb5747a253132000)
Version: 0
ID: C7CC5FA26A039CFA03B32FA56414DFCE19BA318C
Readonly: false
Type: 0x2::Coin::Coin<0x2::GAS::GAS>
```
The result shows some basic information about the object, the owner,
version, ID, if the object is immutable and the type of the object.
If you need a deeper look into the object, you can use the `--json`
flag to view the raw JSON representation of the object.

Here is an example:
```json
{"contents":{"fields":{"id":{"fields":{"id":{"fields":{"id":{"fields":{"bytes":"c7cc5fa26a039cfa03b32fa56414dfce19ba318c"},"type":"0x2::ID::ID"}},"type":"0x2::ID::UniqueID"},"version":0},"type":"0x2::ID::VersionedID"},"value":100000},"type":"0x2::Coin::Coin<0x2::GAS::GAS>"},"owner":{"AddressOwner":[9,153,253,158,238,58,213,87,17,33,130,231,203,87,71,162,83,19,32,0]},"tx_digest":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}
```

## Transferring objects
If you inspect a newly created account, you would expect the account does not own any object. Let us inspect the fresh account we create in the [Generating a new account](#generating-a-new-account) section (`F456EBEF195E4A231488DF56B762AC90695BE2DD`):

```shell
$ wallet --no-shell objects --address F456EBEF195E4A231488DF56B762AC90695BE2DD
Showing 0 results.

```
To add objects to the account, you can [invoke a move function](#calling-move-code),
or you can transfer one of the existing objects from the genesis account to the new account using a dedicated wallet command.
We will explore how to transfer objects using the wallet in this section.

`transfer` command usage:
```shell
USAGE:
    transfer [FLAGS] --gas <gas> --object-id <object-id> --to <to>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --gas <gas>                ID of the gas object for gas payment, in 20 bytes Hex string
        --object-id <object-id>    Object to transfer, in 20 bytes Hex string
        --to <to>                  Recipient address
```
To transfer an object to a recipient, you will need the recipient's address,
the object ID of the object that you want to transfer,
and the gas object' ID for the transaction fee payment.

Here is an example transfer of an object to account `F456EBEF195E4A231488DF56B762AC90695BE2DD`.
```shell
$ wallet --no-shell transfer --to F456EBEF195E4A231488DF56B762AC90695BE2DD --object-id 9495C4EEEB6F935A2AA19D9BA5B3D1D47A30F32E --gas 531AE72F84014918704DF57DA990D08EFCA8BF02
Signed Authorities : [k#643e29cb3a426b08ba54752e932d80222843a3fc3a4818d867dbcc59605f9654, k#605313e105007511a1e337ab6577b03d63b73d2d1bd16604033739ab70ac9036, k#5bbb9e8e399c80fbd02cb020487c7ff5c2867969bb690ba0edfd7d80928e2911]
Transaction Kind : Transfer
Recipient : F456EBEF195E4A231488DF56B762AC90695BE2DD
Object ID : 9495C4EEEB6F935A2AA19D9BA5B3D1D47A30F32E
Sequence Number : SequenceNumber(0)
Object Digest : 5236389cfe8ae26f4d0f62bfa8b8e579bc31ce85446a86aafcc8ac20aa04c3e7
----- Transaction Effects ----
Status : Success { gas_used: 18 }
Mutated Objects:
531AE72F84014918704DF57DA990D08EFCA8BF02 SequenceNumber(1) o#ad159e9b5de7d5048d248a2cf079e2d6862151599769df5223947c822f6bc3d2
9495C4EEEB6F935A2AA19D9BA5B3D1D47A30F32E SequenceNumber(1) o#d8cd8a7b39c9a6fce78a1c6afbc4d0eff6f3890937d3ac86937c31e53bb439d7
```

The account will now have 1 object
```shell
$ wallet --no-shell objects --address F456EBEF195E4A231488DF56B762AC90695BE2DD
Showing 1 results.
(9495C4EEEB6F935A2AA19D9BA5B3D1D47A30F32E, SequenceNumber(1), o#d8cd8a7b39c9a6fce78a1c6afbc4d0eff6f3890937d3ac86937c31e53bb439d7)
```

## Merging and splitting coin objects
Overtime, the account might receive coins from other accounts and will become unmanageable when
the number of coins grows; contrarily, the account might need to split the coins for payment or
for transfer to another account.

We can use the `merge-coin` command and `split-coin` command to consolidate or split coins, respectively.

### Merge coins
Usage of `merge-coin`:
```shell
USAGE:
    merge-coin [FLAGS] --coin-to-merge <coin-to-merge> --gas <gas> --gas-budget <gas-budget> --primary-coin <primary-coin>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --coin-to-merge <coin-to-merge>    Coin to be merged, in 20 bytes Hex string
        --gas <gas>                        ID of the gas object for gas payment, in 20 bytes Hex string
        --gas-budget <gas-budget>          Gas budget for this call
        --primary-coin <primary-coin>      Coin to merge into, in 20 bytes Hex string
```
Here is an example of how to merge coins. To merge coins, you will need at lease three coin objects -
two coin objects for merging, and one for the gas payment.
You also need to specify the maximum gas budget that should be expanded for the coin merge operations.
Let us examine objects owned by address `FF4480C3BB1E1B15CF245667B8448D930D2A05BB`
and use the first coin (gas) object as the one to be the result of the merge, the second one to be merged, and the third one to be used as payment:

```shell
$ wallet --no-shell objects --address FF4480C3BB1E1B15CF245667B8448D930D2A05BB
Showing 5 results.
(0ED2A1CE2D7B48141600FF58BD3F9250640B74CA, SequenceNumber(0), o#f402be062cd514366f7ccb7ac530b50ca554c6f1c573c25aff169a059423265e)
(860F49A96D44A7F0F3459C327A8F77C0A51E7365, SequenceNumber(0), o#5a8a8dbf3250f9b9d21b7aff98e51d6d6110fa02d24bcb06b77fbb8c8140f410)
(B9161506A61E9124118EAD41E671756E0CD74A41, SequenceNumber(0), o#81628cc50ee4cc950bec221772863248352e7bc93abd025ea7ec75b4ed4342a0)
(D3AB294E4798062AE2F78945D1820B34B8EC7864, SequenceNumber(0), o#c8ca7b9cabf835ebd9e9f9dad3ab650103d7469e3db848a4b1280300c1acbf4b)
(DCC12F855DC125391DBCD03D437D0789162F03C3, SequenceNumber(0), o#3480deff2a249ee4842258df4b715aaf00b0a51da9965815d837e696e7c32b43)

$ wallet --no-shell merge-coin --primary-coin 0ED2A1CE2D7B48141600FF58BD3F9250640B74CA  --coin-to-merge 860F49A96D44A7F0F3459C327A8F77C0A51E7365 --gas B9161506A61E9124118EAD41E671756E0CD74A41 --gas-budget 1000
----- Certificate ----
Signed Authorities : [k#5bbb9e8e399c80fbd02cb020487c7ff5c2867969bb690ba0edfd7d80928e2911, k#1b3386983513bc17c0cbac9ff78a3b472dcdaf54b261a03f85d061df2350d6d9, k#605313e105007511a1e337ab6577b03d63b73d2d1bd16604033739ab70ac9036]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : join
Object Arguments : [(0ED2A1CE2D7B48141600FF58BD3F9250640B74CA, SequenceNumber(0), o#f402be062cd514366f7ccb7ac530b50ca554c6f1c573c25aff169a059423265e), (860F49A96D44A7F0F3459C327A8F77C0A51E7365, SequenceNumber(0), o#5a8a8dbf3250f9b9d21b7aff98e51d6d6110fa02d24bcb06b77fbb8c8140f410)]
Pure Arguments : []
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("GAS"), name: Identifier("GAS"), type_params: [] })]
----- Merge Coin Results ----
Updated Coin : Coin { id: 0ED2A1CE2D7B48141600FF58BD3F9250640B74CA, value: 200000 }
Updated Gas : Coin { id: B9161506A61E9124118EAD41E671756E0CD74A41, value: 99996 }
```

### Split coins
Usage of `split-coin`:
```shell
USAGE:
    split-coin [FLAGS] [OPTIONS] --coin-id <coin-id> --gas <gas> --gas-budget <gas-budget>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --amounts <amounts>...       Amount to split out from the coin
        --coin-id <coin-id>          Coin to Split, in 20 bytes Hex string
        --gas <gas>                  ID of the gas object for gas payment, in 20 bytes Hex string
        --gas-budget <gas-budget>    Gas budget for this call
```
For splitting coins, you will need at lease two coins to execute the `split-coin` command,
one coin to split, one for the gas payment.

Let us examine objects owned by address `8F89E566BFB2F68DE0DB8E64F8335D957792A7E8`:
```shell
$ wallet --no-shell objects --address 8F89E566BFB2F68DE0DB8E64F8335D957792A7E8
Showing 5 results.
(08B067AE3389E24EDF2E895850504AAF8C482BD5, SequenceNumber(0), o#3f6b7934f0aadca3f9159acb87473eac4e76ddccb6a89c27bd217c5b0545a727)
(23623449E5F4350137C8C9C1207919FB3E6EEB82, SequenceNumber(0), o#b62d59fad007a8011b6c2b706d1ccb8204f65a278fabc31fa6176e682e3dce66)
(2A28437D19558E86DD94EC56D400AD40E9FEE707, SequenceNumber(0), o#1ea8348031d993406c26250cb8dec07763183ba7a270230cbdd41ef7759c05ab)
(D437DC6CC1C724AF457C7271D0C0CBA55BCD1E66, SequenceNumber(0), o#fea9ee46c145fc04dfd0130962d9963a99fc3928edebd97db71b276c6e0bb7a8)
(F6E964C7856DAE054A99761213E3BB2F1717F37D, SequenceNumber(0), o#90143cc803a9c678cc1720c1d14eb4d1e06d6160ebbb0b98add48d6231306cf2)
```

Here is an example of splitting coins, we are splitting out three new coins from the original coin (first one on the list above),
with values of 1000, 5000 and 3000 respectively; note that the `--amounts` argument accepts list of values.
We use the second coin on the list to pay for this transaction.

```shell
$ wallet --no-shell split-coin --coin-id 08B067AE3389E24EDF2E895850504AAF8C482BD5 --amounts 1000 5000 3000 --gas 23623449E5F4350137C8C9C1207919FB3E6EEB82 --gas-budget 1000
----- Certificate ----
Signed Authorities : [k#1b3386983513bc17c0cbac9ff78a3b472dcdaf54b261a03f85d061df2350d6d9, k#643e29cb3a426b08ba54752e932d80222843a3fc3a4818d867dbcc59605f9654, k#5bbb9e8e399c80fbd02cb020487c7ff5c2867969bb690ba0edfd7d80928e2911]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : split_vec
Object Arguments : [(08B067AE3389E24EDF2E895850504AAF8C482BD5, SequenceNumber(0), o#3f6b7934f0aadca3f9159acb87473eac4e76ddccb6a89c27bd217c5b0545a727)]
Pure Arguments : [[3, 232, 3, 0, 0, 0, 0, 0, 0, 136, 19, 0, 0, 0, 0, 0, 0, 184, 11, 0, 0, 0, 0, 0, 0]]
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("GAS"), name: Identifier("GAS"), type_params: [] })]
----- Split Coin Results ----
Updated Coin : Coin { id: 08B067AE3389E24EDF2E895850504AAF8C482BD5, value: 91000 }
New Coins : Coin { id: 63B316CDE357C68DA0C2C0097482B67CA4A28678, value: 1000 },
            Coin { id: 9955F2D0970AA88EE98B6D4038821CF32153385A, value: 3000 },
            Coin { id: 9EB3A7D8AAE73F4BE2530EA68224D6C7120E16C8, value: 5000 }
Updated Gas : Coin { id: 23623449E5F4350137C8C9C1207919FB3E6EEB82, value: 99780 }

$ wallet --no-shell objects --address 8F89E566BFB2F68DE0DB8E64F8335D957792A7E8
Showing 8 results.
(08B067AE3389E24EDF2E895850504AAF8C482BD5, SequenceNumber(1), o#cbd92d42bd1dbae0f43cf5660f5cc619e00fd17945382d8df633172c5ce1a2a6)
(23623449E5F4350137C8C9C1207919FB3E6EEB82, SequenceNumber(1), o#f1e8f1387ce5d67744795db457de5115acade40a8472a13e554def0054125a5b)
(2A28437D19558E86DD94EC56D400AD40E9FEE707, SequenceNumber(0), o#1ea8348031d993406c26250cb8dec07763183ba7a270230cbdd41ef7759c05ab)
(63B316CDE357C68DA0C2C0097482B67CA4A28678, SequenceNumber(1), o#37e8e77c3aa3d1c68d8a4eb81e2a9c718181a10e3f2371cdcb3d421c0b356e5a)
(9955F2D0970AA88EE98B6D4038821CF32153385A, SequenceNumber(1), o#3c1b205a83754f1288e6ae00b126a1fc0693d7e5338bd33606cc8e181bab5295)
(9EB3A7D8AAE73F4BE2530EA68224D6C7120E16C8, SequenceNumber(1), o#8bfb39c83796ddb7d86fba04c142075dad74be84288e2a354bb13e3f949f6290)
(D437DC6CC1C724AF457C7271D0C0CBA55BCD1E66, SequenceNumber(0), o#fea9ee46c145fc04dfd0130962d9963a99fc3928edebd97db71b276c6e0bb7a8)
(F6E964C7856DAE054A99761213E3BB2F1717F37D, SequenceNumber(0), o#90143cc803a9c678cc1720c1d14eb4d1e06d6160ebbb0b98add48d6231306cf2)
```
From the result we can see three new coins were created in the transaction.

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Wallet CLI. Please see our
[Move developer documentation](move.md#first-look-at-move-source-code)
for the first look at Move source code and a description of the
following function we will be calling in this tutorial:

```rust
public fun transfer(c: Coin::Coin<GAS>, recipient: address, _ctx: &mut TxContext) {
    Coin::transfer(c, Address::new(recipient))
}
```

Please note that there is no real need to use a Move call to transfer
objects as this can be accomplish with a built-in wallet
[command](#transferring-objects) - we chose this example due to its
simplicity.


Let us examine objects owned by address `E7EFB976F10753666C821400FD9554B766363317`:

```shell
$ wallet --no-shell objects --address E7EFB976F10753666C821400FD9554B766363317
Showing 5 results.
(591BADC8D906BAE7FEE95D6B6464A474CCC67ACF, SequenceNumber(0), o#379b792beca0da7b5fb9125171f1ad4b92df10e62e0a00f5de167ac84804c268)
(7154ECD49047FC4554D38C41C92DF91736D5A906, SequenceNumber(0), o#66a13b3428ce27490f5480b1f30c189f0b6372ebc4bae10a9323216d941af22e)
(8E2BA960A97B583B58A0B0C2F0B84366A1A9A1B0, SequenceNumber(0), o#e27579a295e8cda8126731a89f31d506493fe4851e71fdf5b59c62013bb88319)
(A43EE4A5F342807AA3E8B8C795F9175117AF77EB, SequenceNumber(0), o#961474b684418520069ee206be8488765c00a03e938ee484902363aee32d6ed9)
(AF1CF17AA1231461BC274DB0CDDCC49E38687667, SequenceNumber(0), o#35916e592f585d9336d4b808afaa89491ca850d11da4270491ea3d949b9040f9)
```

Now that we know which objects are owned by the address starting with,
we can transfer one of them to another address, say one the fresh one
we created in the [Generating a new account](#generating-a-new-account) section
(`F456EBEF195E4A231488DF56B762AC90695BE2DD`). We can try any object,
but for the sake of this exercise, let's choose the last one on the
list.

We will perform the transfer by calling the `transfer` function from
the GAS module using the following Sui Wallet command:

```shell
wallet --no-shell call --function transfer --module GAS --package 0x2 --args \"0x591BADC8D906BAE7FEE95D6B6464A474CCC67ACF\" \"0xF456EBEF195E4A231488DF56B762AC90695BE2DD\" --gas AF1CF17AA1231461BC274DB0CDDCC49E38687667 --gas-budget 1000
```

This is a pretty complicated command so let's explain all of its
parameters one-by-one:

- `--function` - name of the function to be called
- `--module` - name of the module containing the function
- `--package` - ID of the package object where the module containing
  the function is located. (Remember
  that the ID of the genesis Sui package containing the GAS module is
  defined in its manifest file, and is equal to `0x2`.)
- `args` - a list of function arguments:
  - ID of the gas object representing the `c` parameter of the `transfer`
    function
  - address of the new gas object owner
- `--gas` - an object containing gas used to pay for this
  function call
- `--gas-budget` - a decimal value expressing how much gas we are
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
Signed Authorities : [k#5bbb9e8e399c80fbd02cb020487c7ff5c2867969bb690ba0edfd7d80928e2911, k#643e29cb3a426b08ba54752e932d80222843a3fc3a4818d867dbcc59605f9654, k#1b3386983513bc17c0cbac9ff78a3b472dcdaf54b261a03f85d061df2350d6d9]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : GAS
Function : transfer
Object Arguments : [(591BADC8D906BAE7FEE95D6B6464A474CCC67ACF, SequenceNumber(0), o#379b792beca0da7b5fb9125171f1ad4b92df10e62e0a00f5de167ac84804c268)]
Pure Arguments : [[244, 86, 235, 239, 25, 94, 74, 35, 20, 136, 223, 86, 183, 98, 172, 144, 105, 91, 226, 221]]
Type Arguments : []
----- Transaction Effects ----
Status : Success { gas_used: 11 }
Mutated Objects:
591BADC8D906BAE7FEE95D6B6464A474CCC67ACF SequenceNumber(1) o#74fabc9b7b974a277fb7cfc3cf13d694e2a82348cd702fa8c9bb4d47626a91c8
AF1CF17AA1231461BC274DB0CDDCC49E38687667 SequenceNumber(1) o#25c1377fe903fd90ae37ce36e58a1fd16476bec71888ed35e227d3c9e518d9b8
```

This output indicates the gas object whose ID starts with `AF1C`
was updated to collect gas payment for the function call, and the
object whose ID starts with `591B` was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) by querying objects that are now owned by
the sender:

```shell
$ wallet --no-shell objects --address E7EFB976F10753666C821400FD9554B766363317
Showing 4 results.
(7154ECD49047FC4554D38C41C92DF91736D5A906, SequenceNumber(0), o#66a13b3428ce27490f5480b1f30c189f0b6372ebc4bae10a9323216d941af22e)
(8E2BA960A97B583B58A0B0C2F0B84366A1A9A1B0, SequenceNumber(0), o#e27579a295e8cda8126731a89f31d506493fe4851e71fdf5b59c62013bb88319)
(A43EE4A5F342807AA3E8B8C795F9175117AF77EB, SequenceNumber(0), o#961474b684418520069ee206be8488765c00a03e938ee484902363aee32d6ed9)
(AF1CF17AA1231461BC274DB0CDDCC49E38687667, SequenceNumber(1), o#25c1377fe903fd90ae37ce36e58a1fd16476bec71888ed35e227d3c9e518d9b8)
```

We can now see this address no longer owns the object whose ID starts
with `591B`. And if we inspect this object, we can see it has the new
owner, different from the original one
`E7EFB976F10753666C821400FD9554B766363317`:

```shell
$ wallet --no-shell object --id 591BADC8D906BAE7FEE95D6B6464A474CCC67ACF
Owner: AddressOwner(k#f456ebef195e4a231488df56b762ac90695be2dd)
Version: 1
ID: 591BADC8D906BAE7FEE95D6B6464A474CCC67ACF
Readonly: false
Type: 0x2::Coin::Coin<0x2::GAS::GAS>
```

## Publish packages

In order for user-written code to be available in Sui, it must be
_published_ to Sui's [distributed ledger](../learn/how-sui-works.md#architecture).
Please see the [Move developer documentation](move.md) for a
description on how to [write a simple Move code package](move.md#writing-a-package),
which we can publish using Sui wallet's `publish` command.

The publish command
requires us to specify a directory where the user-defined package lives.
It's the path to the `my_move_package` as per the
[package creation description](move.md#writing-a-package)), a gas
object that will be used to pay for publishing the package (we use the
same gas object we used to pay for the function call in the
[Calling Move code](#calling-move-code)) section, and gas budget to put
an upper limit (we use 1000 as our gas budget.

Let us use the same address for publishing that we used for calling Move code in the previous [section](#calling-move-code) (`E7EFB976F10753666C821400FD9554B766363317`) which now has 4 objecst left:

```shell
$ wallet --no-shell objects --address E7EFB976F10753666C821400FD9554B766363317
Showing 4 results.
(7154ECD49047FC4554D38C41C92DF91736D5A906, SequenceNumber(0), o#66a13b3428ce27490f5480b1f30c189f0b6372ebc4bae10a9323216d941af22e)
(8E2BA960A97B583B58A0B0C2F0B84366A1A9A1B0, SequenceNumber(0), o#e27579a295e8cda8126731a89f31d506493fe4851e71fdf5b59c62013bb88319)
(A43EE4A5F342807AA3E8B8C795F9175117AF77EB, SequenceNumber(0), o#961474b684418520069ee206be8488765c00a03e938ee484902363aee32d6ed9)
(AF1CF17AA1231461BC274DB0CDDCC49E38687667, SequenceNumber(1), o#25c1377fe903fd90ae37ce36e58a1fd16476bec71888ed35e227d3c9e518d9b8)
```

The whole command to publish a package for address
`E7EFB976F10753666C821400FD9554B766363317` resembles the following (assuming
that the location of the package's sources is in the `PATH_TO_PACKAGE`
environment variable):

```shell
wallet --no-shell publish --path $PATH_TO_PACKAGE/my_move_package --gas  7154ECD49047FC4554D38C41C92DF91736D5A906 --gas-budget 30000
```

The result of running this command should look as follows:

```shell
----- Certificate ----
Signed Authorities : [k#643e29cb3a426b08ba54752e932d80222843a3fc3a4818d867dbcc59605f9654, k#5bbb9e8e399c80fbd02cb020487c7ff5c2867969bb690ba0edfd7d80928e2911, k#1b3386983513bc17c0cbac9ff78a3b472dcdaf54b261a03f85d061df2350d6d9]
Transaction Kind : Publish
Gas Budget : 1000
----- Transaction Effects ----
Status : Success { gas_used: 571 }
Created Objects:
C9C04F5FE32C9D6609610023BE7F395C18608AD8 SequenceNumber(1) o#f35c3acff5534594112efc84e18c5ac2389edbe15f776e9f39b17cf35dc07861
F01D46F07E740042835AEB522A560AC93B766C19 SequenceNumber(1) o#d6b4c1fff4bd538c5023804d7dbc30a4e49643c2379a4245faa87572db078d62
Mutated Objects:
7154ECD49047FC4554D38C41C92DF91736D5A906 SequenceNumber(1) o#6e25e9c8f6aa0401b12957fa8a57ec215fbf4df3f1fce38fbe4a37e58676ec0e
```

Please note that two objects were created and one object was updated. One of the created objects is an object representing the published package:

```shell
$ wallet --no-shell object --id F01D46F07E740042835AEB522A560AC93B766C19
Owner: SharedImmutable
Version: 1
ID: F01D46F07E740042835AEB522A560AC93B766C19
Readonly: true
Type: Move Package
```
From now on, we can use the package object ID (`F01D46F07E740042835AEB522A560AC93B766C19`) in the Sui wallet's call
command just like we used `0x2` for built-in packages in the
[Calling Move code](#calling-move-code) section.

The updated object is the gas object that was used to pay for
publishing But what is the second create object? The answer to this
question is that the (only) module included in the published package
has an initializer function defined which creates a single
user-defined object (of type `Forge`), as described in the part of
Move developer documentation concerning [module
initializers](move.md#module-initializers).

```shell
$ wallet --no-shell object --id  C9C04F5FE32C9D6609610023BE7F395C18608AD8
Owner: AddressOwner(k#e7efb976f10753666c821400fd9554b766363317)
Version: 1
ID: C9C04F5FE32C9D6609610023BE7F395C18608AD8
Readonly: false
Type: 0xf01d46f07e740042835aeb522a560ac93b766c19::M1::Forge
```

## Customize genesis

The genesis process can be customized by providing a genesis configuration
file using the `--config` flag.

```shell
sui genesis --config <Path to genesis config file>
```
Example `genesis.conf`:
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

All attributes in `genesis.conf` are optional, and default values
will be used if the attributes are not provided.
For example, the
config shown below will create a network of four authorities, and
pre-populate two gas objects for four newly generated accounts:

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
