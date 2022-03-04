# Wallet Quick Start

Welcome to the Sui tutorial on the sample Sui wallet developed
to facilitate local experimentation with Sui features using a
command line interface. In this document, we describe how to set up
Sui wallet and execute wallet commands through its command line
interface, *Wallet CLI*.


## Setup

### Build the binaries

In order to build a Move package and run code defined in this package,
clone the Sui repository to the current directory and build:


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

The `genesis` command creates four authorities and five user accounts
each with five gas objects. These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
such other object transfers or smart contract (Move) calls. These
numbers represent a sample configuration and have been chosen somewhat
arbitrarily; the process of generating the genesis state can be
customized with additional accounts, objects, code, etc. as described
in [Genesis customization](#genesis-customization).

The network configuration is stored in `network.conf` and
can be used subsequently to start the network. `wallet.conf` and `wallet.key` are
also created to be used by the Sui wallet to manage the
newly created accounts.

### View created accounts
The genesis process creates a configuration file `wallet.conf`, and a keystore file `wallet.key` for the
Sui wallet.  The config file contains information of the accounts and
the Sui network. The keystore file contains all the public private key pair of the created accounts.
Sui wallet uses the network information in `wallet.conf` to communicate
with the Sui network authorities  and create transactions using the key
pairs residing in the keystore file.

Here is an example of `wallet.conf` showing the accounts and key pairs
in the wallet configuration (with some values omitted):

```json
{
  "accounts": [
    {
      "address": "a4c0a493ce9879ea06bac93461810cf947823bb3"
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
  "db_folder_path": "./client_db",
  "keystore": {
    "File": "./wallet.key"
  }
}
```
The `accounts` variable contains account's address the wallet manages.
The `authorities` variable contains Sui network authorities' name, host and port information.
It is used to establish connections to the Sui network.

Note `send_timeout`, `recv_timeout` and `buffer_size` are the network
parameters and `db_folder_path` is the path to the account's client state
database. This database stores all the transaction data, certificates
and object data belonging to the account.

#### Key management
The key pairs are stored in `wallet.key`, however, it is not secure 
and shouldn't be used in a production environment. We have plans to 
implement more secure key management and support hardware signing in a future release.

:warning: **Do not use in production**: Keys are stored in file!

### Starting the network

Run the following command to start the local Sui network:

```shell
./sui start 
```

or

```shell
./sui start --config [config file path]
```

The network config file path defaults to `./network.conf` if not
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

The wallet config file path defaults to `./wallet.conf` if not
specified.  

The following commands are supported by the interactive wallet:

    `addresses`      Obtain the addresses managed by the wallet
    `call`           Call a Move function
    `gas`            Obtain all gas objects owned by the address
    `help`           Print this message or the help of the given subcommand(s)
    `new-address`    Generate new address and keypair
    `object`         Get object information
    `objects`        Obtain all objects owned by the address
    `publish`        Publish Move modules
    `sync`           Synchronize client state with authorities
    `transfer`       Transfer an object

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

The result of running this command should resemble the following output:

```shell
Showing 5 results.
0DC70EA8CA82FE6966900C697B73A235B8E2346F
3111A757BE55F195201FD9140DCE55EAEB719D35
4E523C1FAFECE13628161C78521E60AA8B602324
D9573E0B7F73B15C4416DCBD9911CC1A9456CF21
FE574F043D282AAF10B9AE8AB337C74BA8B428C3
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

#### Use `new-address` command to generate new account

To create a new account, execute the `new-address` command in Sui interactive console:

``` shell
sui>-$ new-address
```
The console returns a confirmation after the account has been created resembling:

```
Created new keypair for address : 3F8962C87474F8FB8BFB99151D5F83E677062078
```
  
#### Add existing accounts to `wallet.conf` manually.

If you have existing key pair from an old wallet config, you can copy the account 
address manually to the new `wallet.conf`'s accounts section, and add the keypair to the keystore file, 
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
        --json       Return command outputs in json format
    -V, --version    Prints version information

OPTIONS:
        --address <address>    Address owning the objects
```
To view the objects owned by the accounts created in genesis, run the following command in the Sui interactive console (substitute the address with one of the genesis address in your wallet): 
```shell
sui>-$ objects --address 0DC70EA8CA82FE6966900C697B73A235B8E2346F
```
The result should look similar to the following, which shows the object in the format of (`object_id`, `sequence_number`, `object_hash`).
```shell
Showing 5 results.
(0E4260A6AA1DF29790E76128DC094C030C2D1819, SequenceNumber(0), o#a4ab81b926bb51b64c33fd56fad24a5a33ea4ff8c244349a985c61c7d1a94570)
(70B26102F9DE9A3CC6FF7CB085BA750DA16FDECE, SequenceNumber(0), o#0028abf5225bfcf0a3762996e7c6f54fa7fec00f0526b2ede51f592f49540c30)
(8E306E956CF5C0F058F048A4A00C25BF90AE5A9B, SequenceNumber(0), o#5ae8e7feff1ad501d8ae96bd10fad846e51bd70d4000a284a65eb183b1a1e459)
(9C7626A4CBFFCE894518B8A317F06D051597A378, SequenceNumber(0), o#10dd0b5cabf227952a6e731001d3b57039595225eb188ca6e7ac65bf55ac7c6f)
(AFA6A58082E961E8706FFF48A0D531C2BED8A94D, SequenceNumber(0), o#2c4b5c7c8be3055287deb4d445c87cf02603d84155d761bcd71f0457d76254ad)
```
If you want to view more information about the objects, you can use the `object` command.

Usage of `object` command :
```shell
USAGE:
    object [FLAGS] --id <id>

FLAGS:
    -h, --help       Prints help information
        --json       Return command outputs in json format
    -V, --version    Prints version information

OPTIONS:
        --id <id>    Object ID of the object to fetch
```
To view the object, use the following command:
```bash
object --id 0E4260A6AA1DF29790E76128DC094C030C2D1819
```
This should give you output similar to the following:
```shell
Owner: SingleOwner(k#ebcf32ca2998dc04b29dc6083250408278f96435)
Version: 0
ID: 0E4260A6AA1DF29790E76128DC094C030C2D1819
Readonly: false
Type: 0x2::Coin::Coin<0x2::GAS::GAS>
```
The result shows some basic information about the object, the owner, 
version, id, if the object is immutable and the type of the object.
If you need a deeper look into the object, you can use the `--json`
flag to view the raw json representation of the object.

Here is an example:
```json
{"contents":{"fields":{"id":{"fields":{"id":{"fields":{"id":{"fields":{"bytes":"0e4260a6aa1df29790e76128dc094c030c2d1819"},"type":"0x2::ID::ID"}},"type":"0x2::ID::UniqueID"},"version":0},"type":"0x2::ID::VersionedID"},"value":100000},"type":"0x2::Coin::Coin<0x2::GAS::GAS>"},"owner":{"SingleOwner":[235,207,50,202,41,152,220,4,178,157,198,8,50,80,64,130,120,249,100,53]},"tx_digest":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}
```

## Transfer objects
If you inspect a newly created account, you would expect the account does not own any object.
```shell
sui>-$ new-address
Created new keypair for address : 830F66EA8EA867DDCA479535BC12CE2852E571F2
sui>-$ objects --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Showing 0 results.
```
To add objects to the account, you can invoke a move function (see [Calling Move code](#calling-move-code) for more information), 
or you can transfer one of the existing object from the genesis account to the new account. 
We will explore how to transfer objects using the wallet in this section.

`transfer` command usage:
```shell
USAGE:
    transfer [FLAGS] --gas <gas> --object-id <object-id> --to <to>

FLAGS:
    -h, --help       Prints help information
        --json       Return command outputs in json format
    -V, --version    Prints version information

OPTIONS:
        --gas <gas>                ID of the gas object for gas payment, in 20 bytes Hex string
        --object-id <object-id>    Object to transfer, in 20 bytes Hex string
        --to <to>                  Recipient address
```
To transfer an object to a recipient, you will need the recipient's address, 
the object id of the object that you want to transfer, 
and the gas object' id for the transaction fee payment.

Here is an example of a transfer of object to account `830F66EA8EA867DDCA479535BC12CE2852E571F2`.
```shell
sui>-$ transfer --to 830F66EA8EA867DDCA479535BC12CE2852E571F2 --object-id 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE --gas 27073453E1B556D1B6C2E4BE94FF2A3928D788BF
Transfer confirmed after 10500 us
----- Certificate ----
Signed Authorities : [k#a723959ed6c6d9b4a508fa527c00c215681812b2f0e86c486bbc204ca94f6df9, k#de9739d8d39bd1e9cfd11ab777fdce42ddf3fd862601f6ab09ee7482054e8da0, k#fb942f73e08b2686d0daef41307fd15804bfd953baf497762f36255401d7b2bf]
Transaction Kind : Transfer
Recipient : 830F66EA8EA867DDCA479535BC12CE2852E571F2
Object ID : 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE
Sequence Number : SequenceNumber(0)
Object Digest : 9d0101cbc56fffae871896864910f6eab4d9af51884758d9e4582766314cce54
----- Transaction Effects ----
Status : Success { gas_used: 18 }
Mutated Objects:
1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE SequenceNumber(1) o#5e98374af72fa72f69f51b05eff1298b0f9045998f2d63db65ad1c26153bd5b3
27073453E1B556D1B6C2E4BE94FF2A3928D788BF SequenceNumber(1) o#052a7e8af210c9be7409c5242e60d2d0e3f5f0e933e33b81280e4c5112923715
```

The account will now have 1 object
```shell
sui>-$ sync --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Client state sync complete.

sui>-$ objects --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Showing 1 results.
(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(1), o#5e98374af72fa72f69f51b05eff1298b0f9045998f2d63db65ad1c26153bd5b3)
```

## Merging and splitting coin objects
Overtime, the account might receive coins from other account and will become unmanageable when 
the number of coins grows; contrarily the account might need to split the coin for payment or 
for transfer to other account.

We can use the `merge-coin` command and `split-coin` command to consolidate or split coins.

### Merge coins
Usage of `merge-coin`:
```shell
USAGE:
    merge-coin [FLAGS] --coin-to-merge <coin-to-merge> --gas <gas> --gas-budget <gas-budget> --primary-coin <primary-coin>

FLAGS:
    -h, --help       Prints help information
        --json       Return command outputs in json format
    -V, --version    Prints version information

OPTIONS:
        --coin-to-merge <coin-to-merge>    Coin to be merged, in 20 bytes Hex string
        --gas <gas>                        ID of the gas object for gas payment, in 20 bytes Hex string
        --gas-budget <gas-budget>          Gas budget for this call
        --primary-coin <primary-coin>      Coin to merge into, in 20 bytes Hex string
```
Here is an example of how to merge coins, to merge coins, you will need at lease three coin objects - 
two coin objects for merging, and one for the gas payment.
```shell
sui>-$ objects --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Showing 3 results.
(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(1), o#5e98374af72fa72f69f51b05eff1298b0f9045998f2d63db65ad1c26153bd5b3)
(6A506E6779CF5936145628C689045A25643ACBDC, SequenceNumber(1), o#6f66513613dc5e5f7814c79f6849850e2489628a94094e4e037e4e1397425a0d)
(B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF, SequenceNumber(1), o#5e9289f55bac3508deaeca6d8ab4e9a4a43de5ef4f7d780a1a5a0a6633d85d96)

sui>-$ merge-coin --primary-coin 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE --coin-to-merge 6A506E6779CF5936145628C689045A25643ACBDC --gas B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF --gas-budget 1000
----- Certificate ----
Signed Authorities : [k#de9739d8d39bd1e9cfd11ab777fdce42ddf3fd862601f6ab09ee7482054e8da0, k#a723959ed6c6d9b4a508fa527c00c215681812b2f0e86c486bbc204ca94f6df9, k#fb942f73e08b2686d0daef41307fd15804bfd953baf497762f36255401d7b2bf]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : join
Object Arguments : [(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(1), o#5e98374af72fa72f69f51b05eff1298b0f9045998f2d63db65ad1c26153bd5b3), (6A506E6779CF5936145628C689045A25643ACBDC, SequenceNumber(1), o#6f66513613dc5e5f7814c79f6849850e2489628a94094e4e037e4e1397425a0d)]
Pure Arguments : []
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("GAS"), name: Identifier("GAS"), type_params: [] })]
----- Merge Coin Results ----
Updated Coin : Coin { id: 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, value: 200000 }
Updated Gas : Coin { id: B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF, value: 99996 }
```

### Split coin
Usage of `split-coin`:
```shell
USAGE:
    split-coin [FLAGS] [OPTIONS] --coin-id <coin-id> --gas <gas> --gas-budget <gas-budget>

FLAGS:
    -h, --help       Prints help information
        --json       Return command outputs in json format
    -V, --version    Prints version information

OPTIONS:
        --amounts <amounts>...       Amount to split out from the coin
        --coin-id <coin-id>          Coin to Split, in 20 bytes Hex string
        --gas <gas>                  ID of the gas object for gas payment, in 20 bytes Hex string
        --gas-budget <gas-budget>    Gas budget for this call
```
For splitting coins, you will need at lease two coins to execute the `split-coin` command, 
one coin to split, one for the gas payment.

Here is an example of splitting coin, we are splitting out three new coins from the original coin, 
with values of 1000, 5000 and 3000 respectively, note that the `--amounts` accepts list of values.
```shell
sui>-$ objects --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Showing 2 results.
(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(2), o#1ce25191bf2832df1bda257044f5764c4ec6144dc6d065ef0bec7ec8bd3e1d60)
(B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF, SequenceNumber(2), o#ee32f7158a56efdcfb20ce292f9b6065201f0d9f15dcea67ba3afb572910e3a5)

sui>-$ split-coin --coin-id 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE --amounts 1000 5000 3000 --gas B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF --gas-budget 1000
----- Certificate ----
Signed Authorities : [k#a723959ed6c6d9b4a508fa527c00c215681812b2f0e86c486bbc204ca94f6df9, k#dee2507e5935e836624d66d16817d79426cfbf0a75b39564467463ce619862fa, k#fb942f73e08b2686d0daef41307fd15804bfd953baf497762f36255401d7b2bf]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : split_vec
Object Arguments : [(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(2), o#1ce25191bf2832df1bda257044f5764c4ec6144dc6d065ef0bec7ec8bd3e1d60)]
Pure Arguments : [[3, 232, 3, 0, 0, 0, 0, 0, 0, 136, 19, 0, 0, 0, 0, 0, 0, 184, 11, 0, 0, 0, 0, 0, 0]]
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("GAS"), name: Identifier("GAS"), type_params: [] })]
----- Split Coin Results ----
Updated Coin : Coin { id: 1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, value: 191000 }
New Coins : Coin { id: 2311C83B04D0755390C0FA3DA5B0DBF7AA14FADD, value: 3000 },
            Coin { id: 538D2C507C34BE647A86629CC9509B12FD5330C2, value: 1000 },
            Coin { id: BFA8BAB64ED8F74BA3731C9220FD7462456BC601, value: 5000 }
Updated Gas : Coin { id: B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF, value: 99776 }

sui>-$ objects --address 830F66EA8EA867DDCA479535BC12CE2852E571F2
Showing 5 results.
(1E1D62BDE28964F6FC0CE3503B5058C4DC04F1DE, SequenceNumber(3), o#300ca9a58cbfdca9e3692378753bf0c15026d21fa8f2e9169f09390a101f1097)
(2311C83B04D0755390C0FA3DA5B0DBF7AA14FADD, SequenceNumber(1), o#158383eae6edaa37c3679653eb7edd46431a903397a305e15d0c3adadb7957b1)
(538D2C507C34BE647A86629CC9509B12FD5330C2, SequenceNumber(1), o#d02a23b46168e62525401dbce913105989ebfb9d33045c0e58b8b759b173be29)
(B7FD91A802DAF5523CAAB69FF4652FAFA6FF4ADF, SequenceNumber(3), o#839e98b33b3de62bc84c36c35b9fd6cdf3383f9d7c4f760c398cbbc7bef8c932)
(BFA8BAB64ED8F74BA3731C9220FD7462456BC601, SequenceNumber(1), o#6748854128a8b4746fb5bd124eddafc4f9129bae36a8a34af85a8f29b07ee124)
```
From the result we can see three new coins got created in the transaction.

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

When looking at the output, let's focus on the first column (ignoring
the opening parenthesis), which lists object IDs owned by this address
(the rest of the input is replaced with `...` below):

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

This is a pretty complicated command so let's explain all of its
parameters one-by-one:

- `--function` - name of the function to be called
- `--module` - name of the module containing the function
- `--package` - ID of the package object where the module containing
  the function is located ([remember](#a-quick-look-at-the-gas-module)
  that the ID of the genesis Sui package containing the GAS module is
  defined in its manifest file, and is equal to `0x2`)
- `args` - a list of function arguments:
  - ID of the gas object representing the `c` parameter of the `transfer`
    function
  - address of the new gas object owner
- `--gas` - an object containing gas used to pay for this
  function call owned by the address initiating the `transfer`
  function call (i.e., address starting with `0DC7`) - we chose the gas
  object whose ID starts with `60DA` but we could have selected any object
  owned by this address as at this point the only objects in Sui are
  gas objects
- `--gas-budget` - a decimal value expressing how much gas we are
  willing to pay for the `transfer` call to be completed (the gas
  object may contain much more gas than 1000 units, and we may want to
  prevent it being drained accidentally beyond what we are intended to
  pay)
  
Note the third argument to the `transfer` function representing
`TxContext` does not have to be specified explicitly - it
is a required argument for all functions callable from Sui and is
auto-injected by the platform at the point of a function call.

The output of the call command is a bit verbose, but the important
information that should be printed at the end indicates objects
changes as a result of the function call (we again abbreviate the
output to include only the first column of the object description
containing its ID):

``` shell
...
Mutated Objects:
60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...
F18F5B785D5766CD85BC2247F8C73F07BFF901BB ...
```

This output indicates the gas object whose ID starts with `60DA`
was updated to collect gas payment for the function call, and the
object whose ID starts with `F18F` was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) by querying objects that are now owned by
the sender (abbreviated output):

``` shell
sui>-$ objects --address 0DC70EA8CA82FE6966900C697B73A235B8E2346F
Showing 4 results.
(60DADCE6E5081C3EFCA162694D7EFD8D99D46636 ...
(B216DCFE027479D0BE9D85A5CD7184E9673452D8 ...
(B56269D5C471367BEDEDDFCBE8A9D928E7C1F170 ...
(D9DBDEDB501C63996E2662DDD23A76A642E8160B ...
```

We can now see this address no longer owns the object whose ID
starts with `F18F`. And if we inspect this object, we can see
it has the new owner:

``` shell
sui>-$ object --id F18F5B785D5766CD85BC2247F8C73F07BFF901BB
Owner: SingleOwner(k#3111a757be55f195201fd9140dce55eaeb719d35)
Version: 1
ID: F18F5B785D5766CD85BC2247F8C73F07BFF901BB
Readonly: false
Type: 0x2::Coin::Coin<0x2::GAS::GAS>
```

## Publish packages

In order for user-written code to be available in Sui, it must be
_published_ to Sui's [distributed
ledger](../../README.md#architecture). Please see the [Move developer
documentation](move.md) for a
[description](move.md#writing-a-package) on how to write a simple Move
code package, which we can publish using Sui wallet's `publish` command.

In order to show how to publish user-defined Move packages, let us
continue where we left off in the previous
[Calling Move code](#calling-move-code) section. The publish command
requires us to specify a directory where the user-defined package lives.
It's the path to the `my_move_package` as per the
[package creation description](move.md#writing-a-package)), a gas
object that will be used to pay for publishing the package (we use the
same gas object we used to pay for the function call in the previous
[calling Move code](#calling-move-code)) section, and gas budget to put
an upper limit (we use 1000 as our gas budget. The whole command resembles:

``` shell
sui>-$ publish --path /PATH_TO_PACKAGE/my_move_package --gas 60DADCE6E5081C3EFCA162694D7EFD8D99D46636 1000
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

From now on, we can use the package object ID in the Sui wallet's call
command just like we used `0x2` for built-in packages in the
[Calling Move code](#calling-move-code) section.

## Customize genesis

The genesis process can be customized by providing a genesis configuration
file using the `--config` flag.

```shell
./sui genesis --config <Path to genesis config file>
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
