---
title: Wallet Quick Start
---

Welcome to the Sui tutorial on the sample Sui wallet developed
to facilitate local experimentation with Sui features using a
command line interface. In this document, we describe how to set up
Sui wallet and execute wallet commands through its command line
interface, *Wallet CLI*.

## Set up

Follow the instructions to [install Sui binaries](install.md#binaries).

## Connect to DevNet
We are hosting a public DevNet for the community to experiment with our 
tech and help to shape the future of Sui network. To connect the wallet 
client to the DevNet, run the following command:
```shell
$ wallet
```
The wallet will print the following line if the wallet is starting up the 
first time. (if you have used the wallet before with a local network, 
follow [here](#manually-changing-the-gateway-url) to change the config for DevNet.)
```shell
Config file ["/Users/dir/.sui/sui_config/wallet.conf"] doesn't exist, do you want to connect to a Sui Gateway [yn]?
```
Type 'y' and then press 'Enter'. You should see the following output.
```shell
Sui Gateway Url (Default to Sui DevNet if not specified) : 
```
The wallet will prompt for the Gateway URL, press 'Enter' and it will default to the DevNet, 
or enter the URL if you want to connect to a Gateway hosted elsewhere.

### Manually changing the Gateway URL
If you have use the wallet before, you will have an existing `wallet.conf`.
You can change the configured Gateway URL to DevNet by using:
```shell
$ wallet switch --gateway http://gateway.devnet.sui.io:9000
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
`wallet new-address` command covered later.

The network configuration is stored in `network.conf` and can be used
subsequently to start the network. The `wallet.conf` and `wallet.key`
are also created to be used by the Sui wallet to manage the newly
created accounts.

By default, these files are placed in your home directory at
`~/.sui/sui_config` (created automatically if it does not yet exist). But you
can override this location by providing an alternative path to the `--working-dir`
argument. Run the command like so to place the files in the `dir` directory:

```shell
$ sui genesis --working-dir /path/to/sui/config/dir
```

:note: That path and directory must already exist and will not be
created with the `--working-dir` argument.

### Recreating genesis

To recreate Sui genesis state in the same location, which will remove
existing configuration files, pass the `--force` option to the `sui
genesis` command and either run it in the default directory (`~/.sui/sui_config`) or specify
it once again, using the `--working-dir` argument:

```shell
$ sui genesis --force --working-dir /path/to/sui/config/dir
```

## Wallet configuration

The genesis process creates a configuration file `wallet.conf`, and a keystore file `wallet.key` for the
Sui wallet.  The config file contains information of the accounts and
the Sui Network Gateway. The keystore file contains all the public-private key pairs of the created accounts.
Sui wallet uses the network information in `wallet.conf` to communicate
with the Sui network validators  and create transactions using the key
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

The `accounts` variable contains the account's address that the wallet manages. The
`gateway` variable contains the information of the Sui network that the wallet will
be connecting to. Currently, only the `embedded` gateway type is supported.

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
You can also connect the wallet to the Sui network via an [RPC Gateway](json-rpc.md#start-local-rpc-server);
To use the RPC gateway, update `wallet.conf`'s `gateway` section to:
```json
{
  ...
  "gateway": {
    "rpc":"http://127.0.0.1:5001"
  },
  ...
}
```

### Key management

The key pairs are stored in `wallet.key`. However, this is not secure
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
`network.conf` in the `~/.sui/sui_config` directory. But you can
override this setting by providing a path to the directory where
this file is stored:

```shell
$ sui start --config /path/to/sui/network/config/file
```

For example:

```shell
$ sui start --config /Users/name/tmp/network.conf
```

Executing any of these two commands in a terminal window will result
in no output but the terminal will be "blocked" by the running Sui
instance (it will not return the command prompt). The command can
also be run in background.

NOTE: For logs, set `RUST_LOG=debug` before invoking `sui start`.

If you see errors when trying to start Sui network, particularly if you made some custom changes
 (e.g,
[customized wallet configuration](#wallet-configuration)), you should [recreate Sui genesis state](#recreating-genesis).

## Using the wallet

Now start a new terminal since you have Sui running in the first terminal.

The following commands are supported by the wallet:

    active-address        Default address used for commands when none specified
    addresses             Obtain the addresses managed by the wallet
    call                  Call Move function
    clear                 Clear screen (interactive only)
    create-example-nft    Create an example NFT
    echo                  Write arguments to the console output (interactive only)
    env                   Print environment (interactive only)
    exit                  Exit the interactive shell (interactive only)
    gas                   Obtain all gas objects owned by the address
    help                  Print this message or the help of the given subcommand(s)
    history               Print history
    merge-coin            Merge two coin objects into one coin
    new-address           Generate new address and keypair
    object                Get object info
    objects               Obtain all objects owned by the address
    publish               Publish Move modules
    split-coin            Split a coin object into multiple coins
    switch                Switch active address
    sync                  Synchronize client state with authorities
    transfer              Transfer an object

> **Note:** The `clear`, `echo`, `env` and `exit` commands exist only in the interactive shell.

Use `wallet -h` to see the most up-to-date list of commands.

Use `help <command>` to see more information on each command.

You can start the wallet in two modes: interactive shell or command line interface.

### Interactive shell

To start the interactive shell, execute the following (in a different
terminal window than one used to execute `sui start`). Assuming you
accepted the default location for configuration:

```shell
$ wallet -i
```

This command will look for the wallet configuration file
`wallet.conf` in the `~/.sui/sui_config` directory. But you can
override this setting by providing a path to the directory where this
file is stored:

```shell
$ wallet -i --config /path/to/wallet/config/file
```

The Sui interactive wallet supports the following shell functionality:

* *Command history* -
  The `history` command can be used to print the interactive shell's command history;
  you can also use Up, Down or Ctrl-P, Ctrl-N to navigate previous or next matches from history.
  History search is also supported using Ctrl-R.
* *Tab completion* -
  Tab completion is supported for all commands using Tab and Ctrl-I keys.
* *Environment variable substitution* -
  The wallet shell will substitute inputs prefixed with `$` with environment variables,
  you can use the `env` command to print out the entire list of variables and
  use `echo` to preview the substitution without invoking any commands.

### Command line mode

The wallet can also be used without the interactive shell, which can be useful if
you want to pipe the output of the wallet to another application or invoke wallet
commands using scripts.

```shell
USAGE:
    wallet [SUBCOMMAND]
```

For example, we can use the following command to see the list of
accounts available on the platform:

```shell
$ wallet addresses
```

The result of running this command should resemble the following output:

```shell
Showing 5 results.
66AF3898E7558B79E115AB61184A958497D1905A
AE6FB6036570FEC1DF71599740C132CDF5B45B9D
45CDA12E3BAFE3017B4B3CD62C493E5FBAAD7FB0
EF999DBDB19CCCA504EEF5432CEC69EA8A1D4A1B
4489AB46A230C1876578441D68F25BF968E6F2B0
```

But the actual address values will most likely differ
in your case (as will other values, such as object IDs, in the later
parts of this tutorial). Consequently, **do not copy and paste
the actual command from this tutorial as they are unlikely to work for
you verbatim**. Each time you create a config for the wallet, addresses
and object IDs will be assigned randomly. Consequently, you cannot rely
on copy-pasting commands that include these values, as they will be different
between different users/configs.

### Active address

Since a wallet manages multiple disjointed addresses, one might need to specify
which address they want to call a command on.

For convenience, one can choose to set a default, or active address that will be
used for commands that require an address to operate on. A default address is picked
at the start, but this can be changed later.

In order to see what the current active address is, use the command `active-address`

```shell
$ wallet active-address
```

Which will reveal and address resembing:

```shell
562F07CF6369E8D22DBF226A5BFEDC6300014837
```

Changing the default address is as easy as calling the `switch` command:

```shell
$ wallet switch --address 913CF36F370613ED131868AC6F9DA2420166062E
```

You will see out like:

```shell
Active address switched to 913CF36F370613ED131868AC6F9DA2420166062E
```

One can call, for example, the `objects` command with or without an address specified.
When not specified, the active address is used.

```
sui>-$ objects
Showing 5 results.
(0B8A4620426E526FA42995CF26EB610BFE6BF063, SequenceNumber(0), o#6ea7e2d4bf47b3cc219fdc44bf15530244d3b3d1838d59586c0bb41d3db92221)

sui>-$ objects --address 913CF36F370613ED131868AC6F9DA2420166062E
Showing 5 results.
(0B8A4620426E526FA42995CF26EB610BFE6BF063, SequenceNumber(0), o#6ea7e2d4bf47b3cc219fdc44bf15530244d3b3d1838d59586c0bb41d3db92221)
```

All commands where `address` is omitted will now use the newly specified active address:
913CF36F370613ED131868AC6F9DA2420166062E

Note that if one calls a command that uses a gas object not owned by the active address,
the address owned by the gas object is temporarily used for the transaction.

### Paying For transactions with gas objects

All Sui transactions require a gas object for payment, as well as a budget. However, specifying
the gas object can be cumbersome; so in the CLI, one is allowed to omit the gas object and leave
the wallet to pick an object that meets the specified budget. This gas selection logic is currently
rudimentary as it does not combine/split gas as needed but currently picks the first object it finds
that meets the budget. Note that one can always specify their own gas if they want to manage the gas
themselves.

:warning: A gas object cannot be part of the transaction while also being used to
pay for the transaction. For example, one cannot try to transfer gas object X while paying for the
transaction with gas object X. The gas selection logic checks for this and rejects such cases.

To see how much gas is in an account, use the `gas` command. Note that this command uses the
`active-address`, unless otherwise specified.

```shell
$ wallet gas
```

You will see output like:

```
                Object ID                 |  Version   |  Gas Value
----------------------------------------------------------------------
 0B8A4620426E526FA42995CF26EB610BFE6BF063 |     0      |   100000
 3C0763CCDEA4FF5A4557505A62AB5E1DAF91F4A2 |     0      |   100000
 45A589A9E760D7F75D399327AC0FCBA21495C22E |     0      |   100000
 4C377A3A9D4B1B9C92189DD12BB1DCD0302A954B |     0      |   100000
 F2961464AC6860A05D21B48C020B7E121399965C |     0      |   100000
```

If one does not want to use the active address, the addresses can be specified:

```shell
$ wallet gas --address 562F07CF6369E8D22DBF226A5BFEDC6300014837
                Object ID                 |  Version   |  Gas Value
----------------------------------------------------------------------
 A8DDC2661A19010E5F85CBF6D905DDFBE4DD0320 |     0      |   100000
 B2683D0B592E5B002D110989A52943BC9DA19158 |     0      |   100000
 B41BF45B01C9BEFCE3A0A371E2B98E062691438D |     0      |   100000
 BA9E10F319182F3BD584EDB92C7899CC6D018723 |     0      |   100000
 F8BFE77A5B21E7ABFA3BC285991F9DA4E5CC2D7B |     0      |   100000

```

## Adding accounts to the wallet

Sui's genesis process will create five accounts by default; if that's
not enough, there are two ways to add accounts to the Sui wallet if needed.

### Generating a new account

To create a new account, execute the `new-address` command:

```shell
$ wallet new-address
```

The output shows a confirmation after the account has been created:

```
Created new keypair for address : C72CF3ADCC4D11C03079CEF2C8992AEA5268677A
```

### Add existing accounts to `wallet.conf` manually

If you have an existing key pair from an old wallet config, you can copy the account
address manually to the new `wallet.conf`'s accounts section, and add the key pair to the keystore file;
you won't be able to mutate objects if the account key is missing from the keystore.

Restart the Sui wallet after the modification; the new accounts will appear in the wallet if you query the addresses.

## View objects owned by the account

You can use the `objects` command to view the objects owned by the address.

`objects` command usage:

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
$ wallet objects --address 66AF3898E7558B79E115AB61184A958497D1905A
```

The result should resemble the following, which shows the object in the format of (`object_id`, `sequence_number`, `object_hash`).

```shell
Showing 5 results.
(00A0A5211F6EDCF4BA09D23B8A7250072BE1EDB6, SequenceNumber(0), o#fbb33b6524d4a648fd5fff8dc93f3d6858945959b710a0893c2b86504b38f731)
(054C8263C73ABD697A0F5AA8990D6D7668CE3D0D, SequenceNumber(0), o#cb99c4b8bb83a0b0111583cd2671f27d6eaeb89f89fd7ae822dc335f1a09e187)
(804AEAA287A7F87DD22A0885BD9E09AFF71F1033, SequenceNumber(0), o#3a7684039086ad33ea313f37d21ddaedd1cd95ed1f9564a61ba18f8e81ea017b)
(DA2237A9890BCCEBEEEAE0D23EC739F00D2CE2B1, SequenceNumber(0), o#db58b72bd45fb8331558a01baec42ad1575c5870bee882be5bae29c91856fe74)
(EEA4167BE074537F4A2879C7781D8EF4FFD651CC, SequenceNumber(0), o#ded63e5faac3953b25d55634a3471a27696f4886a293c7c6812123784548b7d4)
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
$ wallet object --id EEA4167BE074537F4A2879C7781D8EF4FFD651CC
```

This should give you output similar to the following:

```shell
Owner: AddressOwner(k#66af3898e7558b79e115ab61184a958497d1905a)
Version: 0
ID: EEA4167BE074537F4A2879C7781D8EF4FFD651CC
Readonly: false
Type: 0x2::Coin::Coin<0x2::SUI::SUI>
```

The result shows some basic information about the object, the owner,
version, ID, if the object is immutable and the type of the object.
If you need a deeper look into the object, you can use the `--json`
flag to view the raw JSON representation of the object.

Here is an example:

```json
{"contents":{"fields":{"id":{"fields":{"id":{"fields":{"id":{"fields":{"bytes":"eea4167be074537f4a2879c7781d8ef4ffd651cc"},"type":"0x2::ID::ID"}},"type":"0x2::ID::UniqueID"},"version":0},"type":"0x2::ID::VersionedID"},"value":100000},"type":"0x2::Coin::Coin<0x2::SUI::SUI>"},"owner":{"AddressOwner":[102,175,56,152,231,85,139,121,225,21,171,97,24,74,149,132,151,209,144,90]},"tx_digest":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}
```

## Transferring objects

If you inspect a newly created account, you would expect the account does not own any object. Let us inspect the fresh account we create in the [Generating a new account](#generating-a-new-account) section (`C72CF3ADCC4D11C03079CEF2C8992AEA5268677A`):

```shell
$ wallet objects --address C72CF3ADCC4D11C03079CEF2C8992AEA5268677A
Showing 0 results.

```

To add objects to the account, you can [invoke a Move function](#calling-move-code),
or you can transfer one of the existing objects from the genesis account to the new account using a dedicated wallet command.
We will explore how to transfer objects using the wallet in this section.

`transfer` command usage:

```shell
USAGE:
    transfer [FLAGS] [OPTIONS] --gas-budget <gas-budget> --object-id <object-id> --to <to>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --gas <gas>                  ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
                                     object with at least gas_budget value will be selected
        --gas-budget <gas-budget>    Gas budget for this transfer
        --object-id <object-id>      Object to transfer, in 20 bytes Hex string
        --to <to>                    Recipient address
```

To transfer an object to a recipient, you will need the recipient's address,
the object ID of the object that you want to transfer,
and optionally the gas object ID for the transaction fee payment. If a gas
object is not specified, one that meets the budget is picked. Gas budget sets a
cap for how much gas you want to spend. We are still finalizing our gas metering
mechanisms. For now, just set something large enough.

Here is an example transfer of an object to account `F456EBEF195E4A231488DF56B762AC90695BE2DD`:

```shell
$ wallet transfer --to C72CF3ADCC4D11C03079CEF2C8992AEA5268677A --object-id DA2237A9890BCCEBEEEAE0D23EC739F00D2CE2B1 --gas-budget 100
```

With output like:

```
Transfer confirmed after 4412 us
----- Certificate ----
Signed Authorities : [k#21d89c3a12409b7aeadf36a9753417ead5fa9ea607ccb666e83b739b8a73c5e8, k#8d86bef2f8ae835d4763c9a697ad5c458130907996d59adc4ea5be37f2e0fab2, k#f9664056f3cc46b03e86beeb3febf99af1c9ec3f6aa709a1dbd101c9e9a79c3a]
Transaction Kind : Transfer
Recipient : C72CF3ADCC4D11C03079CEF2C8992AEA5268677A
Object ID : DA2237A9890BCCEBEEEAE0D23EC739F00D2CE2B1
Sequence Number : SequenceNumber(0)
Object Digest : db58b72bd45fb8331558a01baec42ad1575c5870bee882be5bae29c91856fe74

----- Transaction Effects ----
Status : Success { gas_used: 18, results: [] }
Mutated Objects:
00A0A5211F6EDCF4BA09D23B8A7250072BE1EDB6 SequenceNumber(1) o#0a4be8bae4e4ea4d8e3a9f5d4ff8533aa36bff247238ab668edc1e5369843c64
DA2237A9890BCCEBEEEAE0D23EC739F00D2CE2B1 SequenceNumber(1) o#f77edd77f5c154a850078b81b320870890bbb4f06d18f80fd512b1cc26bc3297
```

The account will now have one object:

```shell
$ wallet objects --address C72CF3ADCC4D11C03079CEF2C8992AEA5268677A
Showing 1 results.
(DA2237A9890BCCEBEEEAE0D23EC739F00D2CE2B1, SequenceNumber(1), o#f77edd77f5c154a850078b81b320870890bbb4f06d18f80fd512b1cc26bc3297)
```

## Creating example NFTs

You may create an [NFT-like object](https://github.com/MystenLabs/sui/blob/27dff728a4c9cb65cd5d92a574105df20cb51887/sui_programmability/framework/sources/DevNetNFT.move#L16) on Sui using the following command:

```shell
$ wallet create-example-nft
```

You will see output resembling:

```shell
Successfully created an ExampleNFT:

Owner: AddressOwner(k#66af3898e7558b79e115ab61184a958497d1905a)
Version: 1
ID: 70874F1ABD0A9A0126726A626FF48374F7B2D9C6
Readonly: false
Type: 0x2::DevNetNFT::DevNetNFT
```

The command will invoke the `mint` function in the `DevNetNFT` module, which mints a Sui object with three attributes: name, description, and image URL with [default values](https://github.com/MystenLabs/sui/blob/27dff728a4c9cb65cd5d92a574105df20cb51887/sui/src/wallet_commands.rs#L39) and transfers the object to your address. You can also provide custom values using the following instructions:


`create-example-nft` command usage:

```shell
USAGE:
    wallet create-example-nft [OPTIONS]

OPTIONS:
        --description <DESCRIPTION>    Description of the NFT
        --gas <GAS>                    ID of the gas object for gas payment, in 20 bytes hex string
                                       If not provided, a gas object with at least gas_budget value
                                       will be selected
        --gas-budget <GAS_BUDGET>      Gas budget for this transfer
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
USAGE:
    merge-coin [FLAGS] [OPTIONS] --coin-to-merge <coin-to-merge> --gas-budget <gas-budget> --primary-coin <primary-coin>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --coin-to-merge <coin-to-merge>    Coin to be merged, in 20 bytes Hex string
        --gas <gas>                        ID of the gas object for gas payment, in 20 bytes Hex string If not provided,
                                           a gas object with at least gas_budget value will be selected
        --gas-budget <gas-budget>          Gas budget for this call
        --primary-coin <primary-coin>      Coin to merge into, in 20 bytes Hex string
```

Here is an example of how to merge coins. To merge coins, you will need at lease three coin objects -
two coin objects for merging, and one for the gas payment.
You also need to specify the maximum gas budget that should be expanded for the coin merge operations.
Let us examine objects owned by address `EF999DBDB19CCCA504EEF5432CEC69EA8A1D4A1B`
and use the first coin (gas) object as the one to be the result of the merge, the second one to be merged, and the third one to be used as payment:

```shell
$ wallet objects --address EF999DBDB19CCCA504EEF5432CEC69EA8A1D4A1B
```

And its output:

```
Showing 5 results.
(149A3493C97FAFC696526052FE08E77043D4BE0B, SequenceNumber(0), o#2d50f098c913e1863ece507dcdcd5a291252f6c1df89ec8f16c62b542ac723b5)
(1B19F74AD77A95D7562432F6991AC9EC1EA2C57C, SequenceNumber(0), o#d390dc554759f892a714b2659046f3f47830cd789b3ec1df9d40bd876c3e1352)
(4C21FCC8CA953162877FE740F78D9C109145CC73, SequenceNumber(0), o#18229401e7eb96bc23878e1f33d134e19ea5fd0a031bdb323c83baae4eab7097)
(646902FA947ABF2E125131AF0F3A9D5697C8F884, SequenceNumber(0), o#f0bc58de072c0f028b02a0fe53644a74e5b490652c49471a99ffccb2fbb0e60e)
(BEC3BF567A6E32508C96663A339635DC0FB0095C, SequenceNumber(0), o#cfafb0b086cb2df2e8dfb25d84948a45aa19578c45bbaef98d1d5fbcf266db40)
```

Then we merge:
```shell
$ wallet merge-coin --primary-coin 149A3493C97FAFC696526052FE08E77043D4BE0B  --coin-to-merge 1B19F74AD77A95D7562432F6991AC9EC1EA2C57C --gas-budget 1000
```

With results resembling:

```
----- Certificate ----
Signed Authorities : [k#21d89c3a12409b7aeadf36a9753417ead5fa9ea607ccb666e83b739b8a73c5e8, k#8d86bef2f8ae835d4763c9a697ad5c458130907996d59adc4ea5be37f2e0fab2, k#f9664056f3cc46b03e86beeb3febf99af1c9ec3f6aa709a1dbd101c9e9a79c3a]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : join_
Object Arguments : [(149A3493C97FAFC696526052FE08E77043D4BE0B, SequenceNumber(0), o#2d50f098c913e1863ece507dcdcd5a291252f6c1df89ec8f16c62b542ac723b5), (1B19F74AD77A95D7562432F6991AC9EC1EA2C57C, SequenceNumber(0), o#d390dc554759f892a714b2659046f3f47830cd789b3ec1df9d40bd876c3e1352)]
Pure Arguments : []
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("SUI"), name: Identifier("SUI"), type_params: [] })]

----- Merge Coin Results ----
Updated Coin : Coin { id: 149A3493C97FAFC696526052FE08E77043D4BE0B, value: 200000 }
Updated Gas : Coin { id: 4C21FCC8CA953162877FE740F78D9C109145CC73, value: 99995 }
```

### Split coins

Usage of `split-coin`:

```shell
USAGE:
    split-coin [FLAGS] [OPTIONS] --coin-id <coin-id> --gas-budget <gas-budget>

FLAGS:
    -h, --help       Prints help information
        --json       Returns command outputs in JSON format
    -V, --version    Prints version information

OPTIONS:
        --amounts <amounts>...       Amount to split out from the coin
        --coin-id <coin-id>          Coin to Split, in 20 bytes Hex string
        --gas <gas>                  ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
                                     object with at least gas_budget value will be selected
        --gas-budget <gas-budget>    Gas budget for this call
```

For splitting coins, you will need at lease two coins to execute the `split-coin` command,
one coin to split, one for the gas payment.

Let us examine objects owned by address `45CDA12E3BAFE3017B4B3CD62C493E5FBAAD7FB0`:

```shell
$ wallet objects --address 45CDA12E3BAFE3017B4B3CD62C493E5FBAAD7FB0
```

With output resembling:

```
Showing 5 results.
(13347BD461E8A2B9EE5DE7F6131063A3050A45C4, SequenceNumber(0), o#4ca351cbf507cac8162cb8278a38c1c9cdf4c6d2be05f2bee405da02ce8a4aa1)
(B402F52BA6216A770939E6D4922AE6D6D05C2256, SequenceNumber(0), o#b95d120c36fab571c2389bccf507530a39e0055cdd9e9793aaf4ef691b1b8c96)
(BA280146ECD5F74F5A0F31DE4D1883BC078D3729, SequenceNumber(0), o#edb2c038d6fd258b71d811cfa941216991d3a6bf99a783c90835becd443eb66c)
(BD0C7B951A255B078044EF492099CD6E0ED1FD9B, SequenceNumber(0), o#9a937af506d95bb1ffc77ff8f8cc0fbcc550c566f9b41289e1f17d67fd1b9bf8)
(FC4D67D8C7DB119901EF0A0D4BC9EC61584A0B2D, SequenceNumber(0), o#f1c1ca7cb3ef5f3e2a4fff5ec4ebc657388b1e2142432f66199886904eaf1669)
```

Here is an example of splitting coins. We are splitting out three new coins from the original coin (first one on the list above),
with values of 1000, 5000 and 3000, respectively; note the `--amounts` argument accepts list of values.
We use the second coin on the list to pay for this transaction.

```shell
$ wallet split-coin --coin-id 13347BD461E8A2B9EE5DE7F6131063A3050A45C4 --amounts 1000 5000 3000 --gas-budget 1000
```

You will see output resembling:

```
----- Certificate ----
Signed Authorities : [k#21d89c3a12409b7aeadf36a9753417ead5fa9ea607ccb666e83b739b8a73c5e8, k#22d43b47ab73dc69819d7f3c840c9c24344bbd6b2e3692400d1c083825362865, k#8d86bef2f8ae835d4763c9a697ad5c458130907996d59adc4ea5be37f2e0fab2]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : Coin
Function : split_vec
Object Arguments : [(13347BD461E8A2B9EE5DE7F6131063A3050A45C4, SequenceNumber(0), o#4ca351cbf507cac8162cb8278a38c1c9cdf4c6d2be05f2bee405da02ce8a4aa1)]
Pure Arguments : [[3, 232, 3, 0, 0, 0, 0, 0, 0, 136, 19, 0, 0, 0, 0, 0, 0, 184, 11, 0, 0, 0, 0, 0, 0]]
Type Arguments : [Struct(StructTag { address: 0000000000000000000000000000000000000002, module: Identifier("SUI"), name: Identifier("SUI"), type_params: [] })]

----- Split Coin Results ----
Updated Coin : Coin { id: 13347BD461E8A2B9EE5DE7F6131063A3050A45C4, value: 91000 }
New Coins : Coin { id: 72129FBF3168C37A4DD8EC7EE69DA28D0D4D4636, value: 5000 },
            Coin { id: 821942C9375B644C6FC7531E46A70ACB98FB5180, value: 1000 },
            Coin { id: D2E65E9A3107662F7B6399BD1D82C235CFD8C874, value: 3000 }
Updated Gas : Coin { id: B402F52BA6216A770939E6D4922AE6D6D05C2256, value: 99780 }

$ wallet objects --address 45CDA12E3BAFE3017B4B3CD62C493E5FBAAD7FB0
Showing 8 results.
(13347BD461E8A2B9EE5DE7F6131063A3050A45C4, SequenceNumber(1), o#4f86a454ed9aa482adcbfece78cdd77d491d4e768aa8034af78a237d18e09f9f)
(72129FBF3168C37A4DD8EC7EE69DA28D0D4D4636, SequenceNumber(1), o#247905d1c8eee09b4d3bd02f4229376cd7482705e28ef7ff4ca86774d09c72b8)
(821942C9375B644C6FC7531E46A70ACB98FB5180, SequenceNumber(1), o#51aefcb853df1d24b98b975795e21b90496135e292967f7dee0a8fc12079d3af)
(B402F52BA6216A770939E6D4922AE6D6D05C2256, SequenceNumber(1), o#9a20e2565db46aa371ab7932ab4b35494ef2e6a2251955a326e5f0fea6c0ee00)
(BA280146ECD5F74F5A0F31DE4D1883BC078D3729, SequenceNumber(0), o#edb2c038d6fd258b71d811cfa941216991d3a6bf99a783c90835becd443eb66c)
(BD0C7B951A255B078044EF492099CD6E0ED1FD9B, SequenceNumber(0), o#9a937af506d95bb1ffc77ff8f8cc0fbcc550c566f9b41289e1f17d67fd1b9bf8)
(D2E65E9A3107662F7B6399BD1D82C235CFD8C874, SequenceNumber(1), o#c904eaa7b7cc659bc34beec8e7d5ab2cfc51236d498c12cde0d7542b3b1d8b89)
(FC4D67D8C7DB119901EF0A0D4BC9EC61584A0B2D, SequenceNumber(0), o#f1c1ca7cb3ef5f3e2a4fff5ec4ebc657388b1e2142432f66199886904eaf1669)
```

From the result, we can see three new coins were created in the transaction.

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Wallet CLI. Please see our
[Move developer documentation](move.md#first-look-at-move-source-code)
for the first look at Move source code and a description of the
following function we will be calling in this tutorial:

```rust
public(script) fun transfer(c: Coin::Coin<SUI>, recipient: address, _ctx: &mut TxContext) {
    Coin::transfer(c, Address::new(recipient))
}
```

Please note that there is no real need to use a Move call to transfer
objects as this can be accomplish with a built-in wallet
[command](#transferring-objects) - we chose this example due to its
simplicity.

Let us examine objects owned by address `AE6FB6036570FEC1DF71599740C132CDF5B45B9D`:

```shell
$ wallet objects --address AE6FB6036570FEC1DF71599740C132CDF5B45B9D
Showing 5 results.
(5044DC15D3C71D500116EB026E8B70D0A180F3AC, SequenceNumber(0), o#748fabf1f7f92c8d00b54f5b431fd4e28d9dfd642cc0bc5c48b16dc0efdc58c1)
(749E3EE0E0AC93BFC06ED58972EFE87717A428DA, SequenceNumber(0), o#05efb7971ec89b78fd512913fb6f9bfbd0b5ffd2e99775493f9703ff153b3998)
(98765D1CBC66BDFC443AA60B614427470B266B28, SequenceNumber(0), o#5f1696a263b9c97ba2e50175db0af1052a70943148b697fca98f98781482eba5)
(A9E4FDA731FC888CC536DA62C887C63E9BECBE77, SequenceNumber(0), o#ed2945e8d8a8a6c2f3fdc75a84c6cea2a9d74e2fce90779d6d3955c9416a75a1)
(B6E55F0EB3B820CB848B3BBB6DB4BC34E54F2413, SequenceNumber(0), o#4c6be9267d9aeb43f024c1604c765e3f127f8bc2dc4174a5fea5f26d1f7ed03e)
```

Now that we know which objects are owned by that address,
we can transfer one of them to another address, say the fresh one
we created in the [Generating a new account](#generating-a-new-account) section
(`C72CF3ADCC4D11C03079CEF2C8992AEA5268677A`). We can try any object,
but for the sake of this exercise, let's choose the last one on the
list.

We will perform the transfer by calling the `transfer` function from
the SUI module using the following Sui Wallet command:

```shell
$ wallet call --function transfer --module SUI --package 0x2 --args \"0x5044DC15D3C71D500116EB026E8B70D0A180F3AC\" \"0xF456EBEF195E4A231488DF56B762AC90695BE2DD\" --gas-budget 1000
```

This is a pretty complicated command so let's explain all of its
parameters one-by-one:

* `--function` - name of the function to be called
* `--module` - name of the module containing the function
* `--package` - ID of the package object where the module containing
  the function is located. (Remember
  that the ID of the genesis Sui package containing the GAS module is
  defined in its manifest file, and is equal to `0x2`.)
* `args` - a list of function arguments formatted as
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
Signed Authorities : [k#21d89c3a12409b7aeadf36a9753417ead5fa9ea607ccb666e83b739b8a73c5e8, k#f9664056f3cc46b03e86beeb3febf99af1c9ec3f6aa709a1dbd101c9e9a79c3a, k#8d86bef2f8ae835d4763c9a697ad5c458130907996d59adc4ea5be37f2e0fab2]
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x2
Module : SUI
Function : transfer
Object Arguments : [(5044DC15D3C71D500116EB026E8B70D0A180F3AC, SequenceNumber(0), o#748fabf1f7f92c8d00b54f5b431fd4e28d9dfd642cc0bc5c48b16dc0efdc58c1)]
Pure Arguments : [[244, 86, 235, 239, 25, 94, 74, 35, 20, 136, 223, 86, 183, 98, 172, 144, 105, 91, 226, 221]]
Type Arguments : []

----- Transaction Effects ----
Status : Success { gas_used: 11, results: [] }
Mutated Objects:
5044DC15D3C71D500116EB026E8B70D0A180F3AC SequenceNumber(1) o#6b384c50aa19204f3dd98dd52b39217ff234ed321cc2666b91ba6dadc14bd837
B6E55F0EB3B820CB848B3BBB6DB4BC34E54F2413 SequenceNumber(1) o#227a2127b17bdfd36c1f7982969588c3baea7a96f7019158018be1c4f152db04
```

This output indicates the gas object
was updated to collect gas payment for the function call, and the
transferred object was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) by querying objects that are now owned by
the sender:

```shell
$ wallet objects --address AE6FB6036570FEC1DF71599740C132CDF5B45B9D
Showing 4 results.
(749E3EE0E0AC93BFC06ED58972EFE87717A428DA, SequenceNumber(0), o#05efb7971ec89b78fd512913fb6f9bfbd0b5ffd2e99775493f9703ff153b3998)
(98765D1CBC66BDFC443AA60B614427470B266B28, SequenceNumber(0), o#5f1696a263b9c97ba2e50175db0af1052a70943148b697fca98f98781482eba5)
(A9E4FDA731FC888CC536DA62C887C63E9BECBE77, SequenceNumber(0), o#ed2945e8d8a8a6c2f3fdc75a84c6cea2a9d74e2fce90779d6d3955c9416a75a1)
(B6E55F0EB3B820CB848B3BBB6DB4BC34E54F2413, SequenceNumber(1), o#227a2127b17bdfd36c1f7982969588c3baea7a96f7019158018be1c4f152db04)
```

We can now see this address no longer owns the transferred object.
And if we inspect this object, we can see it has the new
owner, different from the original one:

```shell
$ wallet object --id 5044DC15D3C71D500116EB026E8B70D0A180F3AC
```

Resulting in:

```
Owner: AddressOwner(k#f456ebef195e4a231488df56b762ac90695be2dd)
Version: 1
ID: 5044DC15D3C71D500116EB026E8B70D0A180F3AC
Readonly: false
Type: 0x2::Coin::Coin<0x2::SUI::SUI>
```

## Publish packages

In order for user-written code to be available in Sui, it must be
*published* to Sui's [distributed ledger](../learn/how-sui-works.md#architecture).
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

Let us use the same address for publishing that we used for calling Move code in the previous [section](#calling-move-code) (`AE6FB6036570FEC1DF71599740C132CDF5B45B9D`) which now has 4 objecst left:

```shell
$ wallet objects --address AE6FB6036570FEC1DF71599740C132CDF5B45B9D
```

Outputting:

```
(749E3EE0E0AC93BFC06ED58972EFE87717A428DA, SequenceNumber(0), o#05efb7971ec89b78fd512913fb6f9bfbd0b5ffd2e99775493f9703ff153b3998)
(98765D1CBC66BDFC443AA60B614427470B266B28, SequenceNumber(0), o#5f1696a263b9c97ba2e50175db0af1052a70943148b697fca98f98781482eba5)
(A9E4FDA731FC888CC536DA62C887C63E9BECBE77, SequenceNumber(0), o#ed2945e8d8a8a6c2f3fdc75a84c6cea2a9d74e2fce90779d6d3955c9416a75a1)
(B6E55F0EB3B820CB848B3BBB6DB4BC34E54F2413, SequenceNumber(1), o#227a2127b17bdfd36c1f7982969588c3baea7a96f7019158018be1c4f152db04)
Showing 4 results.
```

The whole command to publish a package for address
`AE6FB6036570FEC1DF71599740C132CDF5B45B9D` resembles the following (assuming
that the location of the package's sources is in the `PATH_TO_PACKAGE`
environment variable):

```shell
$ wallet publish --path $PATH_TO_PACKAGE/my_move_package --gas-budget 30000
```

The result of running this command should look as follows:

```shell
----- Certificate ----
Signed Authorities : [k#21d89c3a12409b7aeadf36a9753417ead5fa9ea607ccb666e83b739b8a73c5e8, k#8d86bef2f8ae835d4763c9a697ad5c458130907996d59adc4ea5be37f2e0fab2, k#22d43b47ab73dc69819d7f3c840c9c24344bbd6b2e3692400d1c083825362865]
Transaction Kind : Publish
Gas Budget : 30000

----- Publish Results ----
The newly published package object: (BAEEF9626CC17311E6A3EE99B44CA453D2CC390F, SequenceNumber(1), o#9bf20104335bcffcaa51e39737206e87df53b6f907afca6117c82818e704968e)
List of objects created by running module initializers:
Owner: AddressOwner(k#ae6fb6036570fec1df71599740c132cdf5b45b9d)
Version: 1
ID: FDEE51771AE2A264D61EB8A7726D43948B278B90
Readonly: false
Type: 0xbaeef9626cc17311e6a3ee99b44ca453d2cc390f::M1::Forge
Updated Gas : Coin { id: 749E3EE0E0AC93BFC06ED58972EFE87717A428DA, value: 99232 }
```

Please note that running this command resulted in creating an object representing the published package.
From now on, we can use the package object ID (`52FA2FF453CFECBA06BB84B3B43147C586960E69`) in the Sui wallet's call
command just like we used `0x2` for built-in packages in the
[Calling Move code](#calling-move-code) section.

Another object created as a result of package publishing is a
user-defined object (of type `Forge`) crated inside initializer
function of the (only) module included in the published package - see
the part of Move developer documentation concerning [module
initializers](move.md#module-initializers) for more details on module
initializers.

Finally, we  see that the the gas object that was used to pay for
publishing was updated as well.

## Customize genesis

The genesis process can be customized by providing a genesis configuration
file using the `--config` flag.

```shell
$ sui genesis --config <Path to genesis config file>
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
config shown below will create a network of four validators, and
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

If you use any custom accounts in `genesis.conf`, ensure you have a corresponding private key in
`wallet.key`. Ensure `wallet.key` is in the working directory of the wallet. If you do not have the private key of the addresses specified, you cannot use custom genesis. *Never share your private keys.* For testing the `genesis.conf` example below, you can use the following private key:

`genesis.conf`

```
{
  "authorities": [
    {},{},{},{}
  ],
  "accounts": [
    {
      "address": "09818AAC3EDF9CF9B006B70C36E7241768B26386",
      "gas_objects": [
        {
          "object_id": "0000000000000000000000000000000000000003",
          "gas_value": 10000000
        }
      ]
    }
  ]
}
```
`wallet.key`
```
[
  "WKk4nT2oyPKbFrFAyepT5wEsummWsA6qdhsqzc6CVC9fvTt3J2u6yy5WuW9B6OU3mkcyPC/4Axstn0BpIhzZNg==",
]
```
