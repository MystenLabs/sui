---
title: Sui Client CLI
---

Learn how to set up, configure, and use the Sui Client Command Line Interface (CLI). You can use the CLI to experiment with Sui features using a command line interface.

## Set up

The SUI Client CLI installs when you install Sui. See the [Install Sui](install.md) topic for prerequisites and installation instructions.

## Using the Sui client

The Sui Client CLI supports the following commands:

| Command | Description |
| --- | --- |
| `active-address` | Default address used for commands when none specified |
| `active-env` | Default environment used for commands when none specified |
| `addresses` | Obtain the Addresses managed by the client |
| `call` | Call Move function |
| `create-example-nft` | Create an example NFT |
| `envs` | List all Sui environments |
| `execute-signed-tx` | Execute a Signed Transaction. This is useful when the user prefers to sign elsewhere and use this command to execute
| `gas` | Obtain all gas objects owned by the address |
| `help` | Print this message or the help of the given subcommand(s) |
| `merge-coin` | Merge two coin objects into one coin |
| `new-address` | Generate new address and keypair with keypair scheme flag {ed25519 or secp256k1} with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1 |
| `new-env` | Add new Sui environment |
| `object` | Get object info |
| `objects` | Obtain all objects owned by the address |
| `pay` | Pay SUI to recipients following specified amounts, with input coins. Length of recipients must be the same as that of amounts |
| `pay_all_sui` | Pay all residual SUI coins to the recipient with input coins, after deducting the gas cost. The input coins also include the coin for gas payment, so no extra gas coin is required |
| `pay_sui` | Pay SUI coins to recipients following following specified amounts, with input coins. Length of recipients must be the same as that of amounts. The input coins also include the coin for gas payment, so no extra gas coin is required |
| `publish` | Publish Move modules|
| `serialize-transfer-sui` | Serialize a transfer that can be signed. This is useful when user prefers to take the data to sign elsewhere
| `split-coin` | Split a coin object into multiple coins |
| `switch` | Switch active address and network(e.g., devnet, local rpc server) |
| `sync` | Synchronize client state with authorities |
| `transfer` | Transfer object |
| `transfer-sui` | Transfer SUI, and pay gas with the same SUI coin object. If amount is specified, transfers only the amount. If not specified, transfers the object. |

> **Note:** The `clear`, `echo`, `env` and `exit` commands exist only in the interactive shell.

Use `sui client -h` to see a list of supported commands.

Use `sui help <command>` to see more information on each command.

You can start the client in two modes: interactive shell or command line interface [Configure Sui client](../build/devnet.md#configure-sui-client).

### Interactive shell

To start the interactive shell:

```shell
$ sui console
```

The console command looks for the client configuration file
`client.yaml` in the `~/.sui/sui_config` directory. But you can
override this setting by providing a path to the directory where this
file is stored:

```shell
$ sui console --config /workspace/config-files
```

The Sui interactive client console supports the following shell functionality:

  * *Command history* - use the `history` command to print the command history. You can also use Up, Down or Ctrl-P, Ctrl-N to display the previous or next in the history list. Use Ctrl-R to search the command history.
  * *Tab completion* - supported for all commands using Tab and Ctrl-I keys.
  * *Environment variable substitution* - the console substitutes input prefixed with `$` with environment variables. Use the `env` command to print out the entire list of variables and use `echo` to preview the substitution without invoking any commands.

### Command line mode

You can use the client without the interactive shell. This is useful if
you want to pipe the output of the client to another application or invoke client
commands using scripts.

```shell
USAGE:
    sui client [SUBCOMMAND]
```

For example, the following command returns the list of
account addresses available on the platform:

```shell
$ sui client addresses
```

### Active address

You can specify an active address or default address to use to execute commands.

Sui sets a default address to use for commands. It uses the active address for commands that require an address. To view the current active address, use the `active-address` command.

```shell
$ sui client active-address
```

The response to the request resembles the following:

```shell
0x562f07cf6369e8d22dbf226a5bfedc6300014837
```

To change the default address, use the `switch` command:

```shell
$ sui client switch --address 0x913cf36f370613ed131868ac6f9da2420166062e
```

The response resembles the following:

```shell
Active address switched to 0x913cf36f370613ed131868ac6f9da2420166062e
```

You can call the `objects` command with or without specifying an address.
Sui uses the active address if you do not specify one.

```shell
$ sui client objects
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
```
```shell
$ sui client objects 0x913cf36f370613ed131868ac6f9da2420166062e
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
```

All subsequent commands that omit `address` use the new active address:
0x913cf36f370613ed131868ac6f9da2420166062e

If you call a command that uses a gas object that is not owned by the active address,
Sui temporarily uses the address that owns the gas object for the transaction.

### Paying For transactions with gas objects

All Sui transactions require a gas object for payment, as well as a budget. However, specifying the gas object can be cumbersome; so in the CLI, one is allowed to omit the gas object and leave the client to pick an object that meets the specified budget. This gas selection logic is currently rudimentary as it does not combine/split gas as needed but currently picks the first object it finds that meets the budget. Note that one can always specify their own gas if they want to manage the gas themselves.

:warning: A gas object cannot be part of the transaction while also being used to
pay for the transaction. For example, one cannot try to transfer gas object X while paying for the
transaction with gas object X. The gas selection logic checks for this and rejects such cases.

To see how much gas is in an account, use the `gas` command. Note that this command uses the `active-address`, unless otherwise specified.

```shell
$ sui client gas
```

You can specify an address to see the amount of gas for that address instead of the active address.

```shell
$ sui client gas 0x562f07cf6369e8d22dbf226a5bfedc6300014837
```

## Create new account addresses

Sui Client CLI includes 1 address by default. To add more, create new addresses for the client with the `new-address` command, or add existing accounts to the client.yaml.

### Create a new account address

```shell
$ sui client new-address ed25519
```

You must specify the key scheme, either `ed25519` or `secp256k1`.

### Add existing accounts to client.yaml

To add existing account addresses to your client, such as from a previous installation, edit the client.yaml file and add the accounts section. You must also add key pair to the keystore file.

Restart the Sui console after you save the changes to the client.yaml file.

## View objects an address owns

Use the `objects` command to view the objects an address owns.

```shell
sui client objects
```

To view the objects for a different address than the active address, specify the address to see objects for.

```shell
$ sui client objects 0x66af3898e7558b79e115ab61184a958497d1905a
```

To view more information about an object, use the `object` command.

```shell
    sui client object <ID>
```

The result shows some basic information about the object, the owner,
version, ID, if the object is immutable and the type of the object.

To view the JSON representation of the object, include `--json` in the command.

```shell
    sui client object <ID> --json
```

## Transfer objects

You can transfer mutable objects you own to another address using the command below

```shell
    sui client transfer [OPTIONS] --to <TO> --object-id <OBJECT_ID> --gas-budget <GAS_BUDGET>

OPTIONS:
        --coin-object-id <OBJECT_ID>
            Object to transfer, in 20 bytes Hex string

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

To transfer an object to a recipient, you need the recipient's address,
the object ID of the object to transfer, and, optionally, the ID of the coin object for the transaction fee payment. If not specified, a coin that meets the budget is picked. Gas budget sets a cap for how much gas to spend. We are still finalizing our gas metering mechanisms. For now, just set something large enough.

```shell
$ sui client transfer --to 0xf456ebef195e4a231488df56b762ac90695be2dd --object-id 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 --gas-budget 100
```

## Create an example NFT

You can add an example NFT to an address using the `create-example-nft` command. The command adds an NFT to the active address.

```shell
$ sui client create-example-nft
```

The command invokes the `mint` function in the `devnet_nft` module, which mints a Sui object with three attributes: name, description, and image URL with [default values](https://github.com/MystenLabs/sui/blob/27dff728a4c9cb65cd5d92a574105df20cb51887/sui/src/wallet_commands.rs#L39) and transfers the object to your address. You can also provide custom values using the following instructions:


`create-example-nft` command usage:

```shell
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


## Merge and split coin objects

You can merge coins to reduce the number of separate coin objects in an account, or split coins to create smaller coin objects to use for transfers or gas payments.

We can use the `merge-coin` command and `split-coin` command to consolidate or split coins, respectively.

### Merge coins

```shell
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

You need at lease three coin objects to merge coins, two coins to merge and one to pay for gas payment. When you merge a coin, you specify maximum gas budget allowed for the merge transaction.

Use the following command to view the objects that the specified address owns.

```shell
$ sui client objects 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
```

Use the IDs returns from the previous command in the `merge-coin` command.

```shell
$ sui client merge-coin --primary-coin 0x1e90389f5d70d7fa6ce973155460e1c04deae194 --coin-to-merge 0x351f08f03709cebea85dcd20e24b00fbc1851c92 --gas-budget 1000
```

### Split coins

```shell
    sui client split-coin [OPTIONS] --coin-id <COIN_ID> --gas-budget <GAS_BUDGET> (--amounts <AMOUNTS>... | --count <COUNT>)

OPTIONS:
        --amounts <AMOUNTS>...       Specific amounts to split out from the coin
        --coin-id <COIN_ID>          Coin to Split, in 20 bytes Hex string
        --count <COUNT>              Count of equal-size coins to split into
        --gas <GAS>                  ID of the gas object for gas payment, in 20 bytes Hex string If
                                     not provided, a gas object with at least gas_budget value will
                                     be selected
        --gas-budget <GAS_BUDGET>    Gas budget for this call
    -h, --help                       Print help information
        --json                       Return command outputs in json format
```

To split a coin you need at least 2 coin objects, one to split and one to pay for gas fees.

Use the following command to view the objects the address owns.
```shell
$ sui client objects 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

Then use the IDs returned in the `split-coin` command.

The following example splits one coin into three coins of different amounts, 1000, 5000, and 3000. The `--amounts` argument accepts a list of values.

```shell
$ sui client split-coin --coin-id 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 --amounts 1000 5000 3000 --gas-budget 1000
```

Use the `objects` command to view the new coin objects.

```
$ sui client objects 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

The following example splits a coin into three equal parts. To split a coin evenly, don't include the `--amount` argument in the command.

```shell
$ sui client split-coin --coin-id 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 --count 3 --gas-budget 1000
```

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Sui CLI. Please see our
[Move developer documentation](move/index.md#first-look-at-move-source-code)
for the first look at Move source code and a description of the
following function we will be calling in this tutorial:

```rust
public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
    transfer::transfer(c, Address::new(recipient))
}
```

Please note that there is no real need to use a Move call to transfer
coins as this can be accomplished with a built-in Sui client
[command](#transferring-coins) - we chose this example due to its
simplicity.

Let us examine objects owned by address `0x48ff0a932b12976caec91d521265b009ad5b2225`:

```shell
$ sui client objects 0x48ff0a932b12976caec91d521265b009ad5b2225
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

> **Important:** If you use a shell that interprets square brackets ([ ]) as special characters (such as the `zsh` shell), you must enclose the brackets in single quotes. For example, instead of `[7,42]` you must use `'[7,42]'`.
>
> Additionally, when you specify a vector of object IDs, you must enclose each ID in double quotes. For example,
> `'["0x471c8e241d0473c34753461529b70f9c4ed3151b","0x53b50e3020a01e1fd6acf832a871feee240183f0"]'`

To gain a deeper view into the object, include the
> `--json` flag in the `sui client` command to see the raw JSON representation
> of the object.


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
$ sui client objects 0x48ff0a932b12976caec91d521265b009ad5b2225
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
$ sui client object 0x471c8e241d0473c34753461529b70f9c4ed3151b
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

You must publish packages to the Sui [distributed ledger](../learn/how-sui-works.md#architecture) for the code you developed to be available in Sui. To publish packages with the Sui client, use the `publish` command.

The publish command requires that you specify the directory where your package lives using the `--path` parameter. The value is the path to the `my_move_package` as per the [package creation description](move/write-package.md). You must also provide a `gas` object to pay for publishing the package and a `gas-budget` value.

Parameters used for the `publish` command example:
* `--path` - Defines the path to the Move package to publish.
* `--gas` - The Coin object used to pay for gas.
* `--gas-budget` - Gas budget for running module initializers. 
* `--verify-dependencies` - Optional flag to have the CLI check that a dependency exists on-chain. 

Refer to the [Move developer documentation](move/index.md) for a
description on how to [write a simple Move code package](move/write-package.md),
which you can then publish using the Sui client `publish` command.

> **Important:** You must remove all calls to functions in the `debug` module from no-test code
> before you can publish the new module (test code is marked with the `#[test]` annotation).

Use the same address for publishing that we used for calling Move code in the previous [section](#calling-move-code) (`0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75`) which now has four objects left:

```shell
$ sui client objects 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
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
$ sui client publish --path $PATH_TO_PACKAGE/my_move_package --gas 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19 --gas-budget 30000 --verify-dependencies
```

The call uses the optional `--verify-dependencies` flag to verify the bytecode for dependencies found at their respective published addresses matches the bytecode you get when compiling that dependency from source code. If the bytecode for a dependency does not match, your package does not publish and you receive an error message indicating which package and module the mismatch was found:

```shell
Local dependency did not match its on-chain version at <address>::<package>::<module>
```

The `--verify-dependencies` flag can fail the publish for other reasons, as well.
* There are modules missing, either in the local version of the dependency or on-chain.
* There's nothing at the address that the dependency points to (it was deleted or never existed).
* The address supplied for the dependency points to an object instead of a package.
* The CLI fails to connect to the node to fetch the package.

If successful, your response resembles the following:

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

Running this command created an object representing the published package.
From now on, use the package object ID (`0xdbcee02bd4eb326122ced0a8540f15a057d82850`) in the Sui client call
command (similar to `0x2` used for built-in packages in the
[Calling Move code](#calling-move-code) section).

Another object created as a result of package publishing is a
user-defined object (of type `Forge`) created inside the initializer
function of the (only) module included in the published package - see
the part of Move developer documentation concerning [module
initializers](move/debug-publish.md#module-initializers) for more details.

You might notice that the gas object that was used to pay for
publishing was updated as well.

> **Important:** If the publishing attempt results in an error regarding verification failure,
> [build your package locally](../build/move/build-test.md#building-a-package) (using the `sui move build` command)
> to get a more verbose error message.

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
