---
title: Sui Client CLI
---

Learn how to set up, configure, and use the Sui Client Command Line Interface (CLI). You can use the CLI to experiment with Sui features using a command line interface.

## Set up

The Sui Client CLI installs when you install Sui. See the [Install Sui](install.md) topic for prerequisites and installation instructions.

## Using the Sui client

The Sui Client CLI supports the following commands:

| Command               | Description                                                                                                                                                                                                                                   |
|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `active-address`      | Default address used for commands when none specified.                                                                                                                                                                                        |
| `active-env`          | Default environment used for commands when none specified.                                                                                                                                                                                    |
| `addresses`           | Obtain the Addresses managed by the client.                                                                                                                                                                                                   |
| `call`                | Call Move function.                                                                                                                                                                                                                           |
| `dynamic-field`       | Query a dynamic field by address.                                                                                                                                                                                                             |
| `envs`                | List all Sui environments.                                                                                                                                                                                                                    |
| `execute-signed-tx`   | Execute a Signed Transaction. This is useful when the user prefers to sign elsewhere and use this command to execute.                                                                                                                         |
| `gas`                 | Obtain all gas objects owned by the address.                                                                                                                                                                                                  |
| `help`                | Print this message or the help of the given subcommand(s).                                                                                                                                                                                    |
| `merge-coin`          | Merge two coin objects into one coin.                                                                                                                                                                                                         |
| `new-address`         | Generate new address and keypair with keypair scheme flag {ed25519 or secp256k1 or secp256r1} with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1 |
| `new-env`             | Add new Sui environment.                                                                                                                                                                                                                      |
| `object`              | Get object information.                                                                                                                                                                                                                       |
| `objects`             | Obtain all objects owned by the address.                                                                                                                                                                                                      |
| `pay`                 | Pay SUI to recipients following specified amounts, with input coins. Length of recipients must be the same as that of amounts.                                                                                                                |
| `pay_all_sui`         | Pay all residual SUI coins to the recipient with input coins, after deducting the gas cost. The input coins also include the coin for gas payment, so no extra gas coin is required.                                                          |
| `pay_sui`             | Pay SUI coins to recipients following specified amounts, with input coins. Length of recipients must be the same as that of amounts. The input coins also include the coin for gas payment, so no extra gas coin is required.                 |
| `profile-transaction` | Profile the gas usage of a transaction.  Outputs a file `gas_profile_{tx_digest}_{unix_timestamp}.json` which can be opened in a flamegraph tool such as speedscope.                                                                          |
| `publish`             | Publish Move modules.                                                                                                                                                                                                                         |
| `replay-transaction`  | Replay a given transaction to view transaction effects. Set environment variable MOVE_VM_STEP=1 to debug.                                                                                                                                     |
| `replay-batch`        | Replay transactions listed in a file.                                                                                                                                                                                                         |
| `replay-checkpoint`   | Replay all transactions in a range of checkpoints.                                                                                                                                                                                            |
| `split-coin`          | Split a coin object into multiple coins.                                                                                                                                                                                                      |
| `switch`              | Switch active address and network.                                                                                                                                                                                                            |
| `transfer`            | Transfer object.                                                                                                                                                                                                                              |
| `transfer-sui`        | Transfer SUI, and pay gas with the same SUI coin object. If amount is specified, transfers only the amount. If not specified, transfers the object.                                                                                           |
| `upgrade`             | Upgrade a Move module.                                                                                                                                                                                                                        |
| `verify-source`       | Verify local Move packages against on-chain packages, and optionally their dependencies.                                                                                                                                                      |
\

**Note:** The `clear`, `echo`, `env`, and `exit` commands exist only in the interactive shell.

Use `sui client -h` to see a list of supported commands.

Use `sui help <command>` to see more information on each command.

You can start the client in two modes: interactive shell or command line interface [Configure Sui client](../build/connect-sui-network.md#configure-sui-client).

### Interactive shell

To start the interactive shell:

```shell
sui console
```

The console command looks for the client configuration file `client.yaml` in the `~/.sui/sui_config` directory. If you have this file stored in a different directory, provide the updated path to the command to override this setting.

```shell
sui console --client.config /workspace/config-files
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
sui client addresses
```

The response resembles the following:

```Showing 5 results.
0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106
0x011a285261b9f8d10a0c7ecb4c0dbe6d396825768dba38c3056809472736e521
0x4ab708d1a4160fa0fdbf359691764e16380444ddb48d2b8856a169594a9baa55
0xa3c00467938b392a12355397bdd3d319cea5c9b8f4fc9c51b46b8e15a807f030
0xa56612ad4f5dbc04c651e8d20f56af3316ee6793335707f29857bacabf9127d0 <=
```

The `<=` indicates the active address.

### Active address

You can specify an active address or default address to use to execute commands.

Sui sets a default address to use for commands. It uses the active address for commands that require an address. To view the current active address, use the `active-address` command.

```shell
sui client active-address
```

The response to the request resembles the following:

```shell
0xa56612ad4f5dbc04c651e8d20f56af3316ee6793335707f29857bacabf9127d0
```

To change the default address, use the `switch` command:

```shell
sui client switch --address 0xa3c00467938b392a12355397bdd3d319cea5c9b8f4fc9c51b46b8e15a807f030
```

The response resembles the following:

```shell
Active address switched to 0xa3c00467938b392a12355397bdd3d319cea5c9b8f4fc9c51b46b8e15a807f030
```

All commands use the active address if you don't specify an `address`.

### Paying For transactions with gas objects

All Sui transactions require a gas object for gas fees. If you don't specify a gas object, Sui uses a gas object with sufficient SUI to cover the gas fee.

You can't use the same gas object as part of a transaction and to pay for the same transaction.
To see how much gas is in an account, use the `gas` command.

```shell
sui client gas
```

Specify an address to check an address other than the active address.

```shell
sui client gas 0x4e049913233eb918c11638af89d575beb99003d30a245ac74a02e26e45cb80ee
```

## Create new account addresses

Sui Client CLI includes 1 address by default. You can create new addresses for the client with the `new-address` command, or add existing accounts to the client.yaml.

### Create a new account address

```shell
sui client new-address secp256k1
```

You must specify the key scheme, one of `ed25519` or `secp256k1` or `secp256r1`.

The command returns a new address and the 24-word recovery phrase for it.

```shell
Created new keypair for address with scheme Secp256k1: [0x338567a5fe29132d68fade5172870d8ac1b607fd00eaace1e0aa42896d7f97d4]
Secret Recovery Phrase : [guilty coast nephew hurt announce speak kiwi travel churn airport universe escape thrive switch lean lab giraffe gospel punch school dance cloud type gift]
```

### Add existing accounts to client.yaml

To add existing account addresses to your client, such as from a previous installation, edit the client.yaml file and add the accounts section. You must also add the key pair to the keystore file.

Restart the Sui console after you save the changes to the client.yaml file.

## View objects an address owns

Use the `objects` command to view the objects an address owns.

```shell
sui client objects
```

The response resembles the following:

```
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x1aa482ad8c6240cda3097a4aa13ad5bfb27bf6052133c01f79c8b4ea0aaa0601 |     1      | OpU8HmueEaLzK6hkNSQkcahG8qo73ag4vJPG+g8EQBs= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d |     1      | lRamSZkLHnfN9mcrkoVzmXwHxE7GnFHNnqe8dzWEUA8= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x51ec7820e82035a5de7b4f3ba2a3813ea099dca1867876f4177a1fa1d1efe022 |     1      | 1NO7XtdmojnOch4gcCsUHDdV1n2bPYv5je83yXd5Suw= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x727b37454ab13d5c1dbb22e8741bff72b145d1e660f71b275c01f24e7860e5e5 |     1      | 9C1lxL45JIxwX35rL69OtAFUf3kz39Dq6jiguVvpCeM= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0xe638c76768804cebc0ab43e103999886641b0269a46783f2b454e2f8880b5255 |     1      | idJrGmd6ZkzJVQeKtu8XlUt2dA397GURgCUXJOLQhxI= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
Showing 5 results.
```

To view the objects for a different address than the active address, specify the address to see objects for.

```shell
sui client objects 0x338567a5fe29132d68fade5172870d8ac1b607fd00eaace1e0aa42896d7f97d4
```

To view more information about an object, use the `object` command and specify the `objectId`.

```shell
sui client object <OBJECT-ID>
```

The result shows some basic information about the object, the owner,
version, ID, if the object is immutable and the type of the object.
```
----- 0x2::coin::Coin<0x2::sui::SUI> (0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d[0x1]) -----
Owner: Account Address ( 0xa3c00467938b392a12355397bdd3d319cea5c9b8f4fc9c51b46b8e15a807f030 )
Version: 0x1
Storage Rebate: 0
Previous Transaction: TransactionDigest(HJ8WdB6536YHD1vgH9DMhVFS7hfgVUhtgotLBFF9Aosz)
----- Data -----
type: 0x2::coin::Coin<0x2::sui::SUI>
balance: 100000000000000
id: 0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d
```

To view the JSON representation of the object, include `--json` in the command.

```shell
sui client object <OBJECT-ID> --json
```

The response resembles the following:
```json
{
  "objectId": "0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d",
  "version": 1,
  "digest": "B2yn9NvfxsPXDadWd5ga9DirurrY3gyu1sYLT169seZk",
  "type": "0x2::coin::Coin<0x2::sui::SUI>",
  "owner": {
    "AddressOwner": "0xa3c00467938b392a12355397bdd3d319cea5c9b8f4fc9c51b46b8e15a807f030"
  },
  "previousTransaction": "HJ8WdB6536YHD1vgH9DMhVFS7hfgVUhtgotLBFF9Aosz",
  "storageRebate": 0,
  "content": {
    "dataType": "moveObject",
    "type": "0x2::coin::Coin<0x2::sui::SUI>",
    "hasPublicTransfer": true,
    "fields": {
      "balance": "100000000000000",
      "id": {
        "id": "0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d"
      }
    }
  }
}
```

## Transfer objects

You can transfer mutable objects you own to another address using the command below

```shell
sui client transfer [OPTIONS] --to <TO> --object-id <OBJECT-ID> --gas-budget <GAS-BUDGET-AMOUNT>

OPTIONS:
        --object-id <OBJECT-ID>
            Object to transfer, in 32 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 32 bytes Hex string If not provided, a gas object with at least gas_budget value will be selected

        --gas-budget <GAS-BUDGET-AMOUNT>
            Gas budget for this transfer

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --to <TO>
            Recipient address
```

To transfer an object to a recipient, you need the recipient's address,
the object ID of the object to transfer, and, optionally, the ID of the coin object for the transaction fee payment. If not specified, the client uses a coin that meets the budget. Gas budget sets a cap for how much gas to spend.

```shell
sui client transfer --to 0xcd2630011f6cb9aef960ed42d95b04e063c44a6143083ef89a35ea02b85c61b7 --object-id 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427 --gas-budget <GAS-BUDGET-AMOUNT>
```

## Merge and split coin objects

You can merge coins to reduce the number of separate coin objects in an account, or split coins to create smaller coin objects to use for transfers or gas payments.

You can use the `merge-coin` command and `split-coin` command to consolidate or split coins, respectively.

### Merge coins

```shell
sui client merge-coin [OPTIONS] --primary-coin <PRIMARY-COIN> --coin-to-merge <COIN-TO-MERGE> --gas-budget <GAS-BUDGET-AMOUNT>

OPTIONS:
        --coin-to-merge <COIN-TO-MERGE>
            Coin to be merged, in 32 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 32 bytes Hex string If not provided, a gas
            object with at least gas_budget value will be selected

        --gas-budget <GAS-BUDGET>
            Gas budget for this call

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --primary-coin <PRIMARY-COIN>
            Coin to merge into, in 32 bytes Hex string
```

You need at least three coin objects to merge coins, two coins to merge and one to pay for gas payment. When you merge a coin, you specify maximum gas budget allowed for the merge transaction.

Use the following command to view the objects that the specified address owns.

```shell
sui client objects 0x8f603d8a00ae87c43dc090e52bffc29a4b312c28ff3afd81c498caffa2a6b768
```

Use the IDs returned from the previous command in the `merge-coin` command.

```shell
sui client merge-coin --primary-coin 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427 --coin-to-merge 0x11af4b844ff94b3fbef6e36b518da3ad4c5856fa686464524a876b463d129760 --gas-budget <GAS-BUDGET-AMOUNT>
```

### Split coins

```shell
sui client split-coin [OPTIONS] --coin-id <COIN-ID> --gas-budget <GAS-BUDGET-AMOUNT> (--amounts <AMOUNTS>... | --count <COUNT>)

OPTIONS:
        --amounts <AMOUNTS>...       Specific amounts to split out from the coin
        --coin-id <COIN-ID>          Coin to Split, in 32 bytes Hex string
        --count <COUNT>              Count of equal-size coins to split into
        --gas <GAS>                  ID of the gas object for gas payment, in 32 bytes Hex string If
                                     not provided, a gas object with at least gas-budget value will
                                     be selected
        --gas-budget <GAS-BUDGET>    Gas budget for this call
    -h, --help                       Print help information
        --json                       Return command outputs in json format
```

To split a coin you need at least 2 coin objects, one to split and one to pay for gas fees.

Use the following command to view the objects the address owns.
```shell
sui client objects 0xcd2630011f6cb9aef960ed42d95b04e063c44a6143083ef89a35ea02b85c61b7
```

Then use the IDs returned in the `split-coin` command.

The following example splits one coin into three coins of different amounts, 1000, 5000, and 3000. The `--amounts` argument accepts a list of values.

```shell
sui client split-coin --coin-id 0x11af4b844ff94b3fbef6e36b518da3ad4c5856fa686464524a876b463d129760 --amounts 1000 5000 3000 --gas-budget <GAS-BUDGET-AMOUNT>
```

Use the `objects` command to view the new coin objects.

```
sui client objects 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

The following example splits a coin into three equal parts. To split a coin evenly, run the command without the `--amount` argument.

```shell
sui client split-coin --coin-id 0x11af4b844ff94b3fbef6e36b518da3ad4c5856fa686464524a876b463d129760 --count 3 --gas-budget <GAS-BUDGET-AMOUNT>
```

## Calling Move code

The genesis state of the Sui platform includes Move code that is
immediately ready to be called from Sui CLI.

```rust
public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
    transfer::transfer(c, Address::new(recipient))
}
```

Please note that there is no real need to use a Move call to transfer
coins as this can be accomplished with a built-in Sui client
[command](#transfer-objects).

```shell
sui client call --function transfer --module sui --package 0x2 --args 0x1b9c00a93345ce5f12bea9ffe04748d6696c30631735193aea95b8f9082c1062 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427 --gas-budget <GAS-BUDGET-AMOUNT>
```

You can also use environment variables:
```shell
export OBJECT_ID=0x1b9c00a93345ce5f12bea9ffe04748d6696c30631735193aea95b8f9082c1062
export RECIPIENT=0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427
```

```shell
echo $OBJECT_ID
echo $RECIPIENT
```

```shell
sui client call --function transfer --module sui --package 0x2 --args $OBJECT_ID $RECIPIENT --gas-budget <GAS-BUDGET-AMOUNT>
```

The command parameters include:
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
* `--gas-budget` - a decimal value expressing how much gas you are
  willing to pay for the `transfer` call to be completed to avoid
  accidental drainage of all gas in the gas payment
* `--type-args` - a list of types to let the Sui Move compiler know how to fill
  in generic type parameters in the called function.
  It is not needed above, but it would be if the function being called were generic.
  See more about this flag below.

Note the third argument to the `transfer` function representing
`TxContext` does not have to be specified explicitly - it
is a required argument for all functions callable from Sui and is
auto-injected by the platform at the point of a function call.

**Important:**

1. If you use a shell that interprets square brackets ([ ]) as special
   characters (such as the `zsh` shell), you must enclose the brackets in single
   quotes. For example, instead of `[7,42]` you must use `'[7,42]'`.

  To include multiple object IDs, enclose the IDs in double quotes. For example,
  `'["0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427","0x11af4b844ff94b3fbef6e36b518da3ad4c5856fa686464524a876b463d129760"]'`

2. The `--type-args` flag is used if the entry function being called is
  generic, i.e. its signature is similar to
  ```
  public entry fun generic<T_1: ..., T_2: ..., ..., T_N: ...>(...) {...}
  ```
  In this case, the flag `--type-args` would need to be used like this:
  ```bash
  --type-args <type_for_t_1> <type_for_t_2> ... <type_for_t_n>
  ```
  where each of the types above can be among the following non-exhaustive list:
  - a primitive type, such as `u8`, `bool`, `address`
  - a composition of primitive types, like `vector<u8>`
  - a non-generic type or struct defined in a published, public Sui Move package, specified thusly:
  `[package-id]::[module-identifier]::[Struct]`.
  An example of such a type would be `0x1234::xy::Zzy`
  - generic types with arbitrarily nested parameters, in the form
  `[package-id]::[module]::[Struct]<T_1, ..., T_n>`, where `T_1`, ..., `T_n` are fully
  qualified, fully instantiated types, and `[package-id]::[module]::[Struct]` is a generic type
  with `n` parameters.
  An example of such a type would be `vector<0x123::foo::Bar<u32, bool>>`

  When, upon calling `sui client call`, an error similar to
  ```
  Error calling module: Failure {
    error: "VMVerificationOrDeserializationError in command 0",
  }
  ```
  occurs, it may help to check if `--type-args` has all the types the function needs.

## Publish packages

You must publish packages to the Sui [distributed ledger](../learn/how-sui-works.md#architecture) for the code you developed to be available in Sui. To publish packages with the Sui client, use the `publish` command.

Refer to the [Move developer documentation](move/index.md) for a
description on how to [write a simple Move code package](move/write-package.md),
which you can then publish using the Sui client `publish` command.

**Important:** You must remove all calls to functions in the `debug` module from no-test code before you can publish the new module (test code is marked with the `#[test]` annotation).

```shell
sui client objects 0x4e049913233eb918c11638af89d575beb99003d30a245ac74a02e26e45cb80ee
```

The whole command to publish a package for address
`0x338567a5fe29132d68fade5172870d8ac1b607fd00eaace1e0aa42896d7f97d4` resembles the following (assuming that the location of the package sources is in the `PATH_TO_PACKAGE`
environment variable):

```shell
sui client publish $PATH_TO_PACKAGE/my_move_package --gas 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427 --gas-budget <GAS-BUDGET-AMOUNT>
```

The publish command accepts the path to your package as an optional positional parameter (`$PATH_TO_PACKAGE/my_move_package` in the previous call). If you do not supply the path, the command uses the current working directory as the default path value. The call also provides the following data:

 * `--gas` - The Coin object used to pay for gas.
 * `--gas-budget` - Gas budget for running module initializers.

When you publish a package, the CLI verifies that the bytecode for dependencies found at their respective published addresses matches the bytecode you get when compiling that dependency from source code. If the bytecode for a dependency does not match, your package does not publish and you receive an error message indicating which package and module the mismatch was found in:

```shell
Local dependency did not match its on-chain version at <address>::<package>::<module>
```

The publish might fail for other reasons, as well, based on dependency verification:

 * There are modules missing, either in the local version of the dependency or on-chain.
 * There's nothing at the address that the dependency points to (it was deleted or never existed).
 * The address supplied for the dependency points to an object instead of a package.
 * The CLI fails to connect to the node to fetch the package.

If your package fails to publish because of an error in dependency verification, you must find and include the correct and verifiable source package for the failing dependency. If you fully understand the circumstances preventing your package from passing the dependency verification, and you appreciate the risk involved with skipping that verification, you can add the `--skip-dependency-verification` flag to the `sui client publish` command to bypass the dependency check.

**Note:** If your package includes unpublished dependencies, you can add the `--with-unpublished-dependencies` flag to the `sui client publish` command to include modules from those packages in the published build.

If successful, your response resembles the following:

```shell
----- Certificate ----
Transaction Hash: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
Transaction Signature: 7Lqy/KQW86Tq81cUxLMW07AQw1S+D4QLFC9/jMNKrau81eABHpxG2lgaVaAh0c+d5ldYhp75SmpY0pxq0FSLBA==@BE/TaOYjyEtJUqF0Db4FEcVT4umrPmp760gFLQIGA1E=
Signed Authorities : [k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#f2e5749a5fc33d45c6f546eb9e53fabf4f17681ba6f697080de9514f4e0d6a75, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac]
Transaction Kind : Publish
----- Publish Results ----
The newly published package object ID: 0x53e4567ccafa5f36ce84c80aa8bc9be64e0d5ae796884274aef3005ae6733809

List of objects created by running module initializers:
----- Move Object (0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427[1]) -----
Owner: Account Address ( 0x338567a5fe29132d68fade5172870d8ac1b607fd00eaace1e0aa42896d7f97d4 )
Version: 1
Storage Rebate: 12
Previous Transaction: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
----- Data -----
type: 0xdbcee02bd4eb326122ced0a8540f15a057d82850::m1::Forge
id: 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427[1]
swords_created: 0

Updated Gas : Coin { id: 0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427, value: 96929 }
```

Running this command created an object representing the published package.
From now on, use the package object ID (`0x53e4567ccafa5f36ce84c80aa8bc9be64e0d5ae796884274aef3005ae6733809`) in the Sui client call
command (similar to `0x2` used for built-in packages in the
[Calling Move code](#calling-move-code) section).

Another object created as a result of package publishing is a
user-defined object (of type `Forge`) created inside the initializer
function of the (only) module included in the published package - see
the part of Move developer documentation concerning [module
initializers](move/debug-publish.md#module-initializers) for more details.

You might notice that the gas object that was used to pay for
publishing was updated as well.

**Important:** If the publishing attempt results in an error regarding verification failure, [build your package locally](../build/move/build-test.md#building-a-package) (using the `sui move build` command) to get a more verbose error message.

## Verify source

Supply a package path to `verify-source` (or run from package root) to have the CLI compile the package and check that all its modules match their on-chain counterparts.

`sui client verify-source ./code/MyPackage`

The default behavior is for the command to verify only the direct source of the package, but you can supply the `--verify-deps` flag to have the command verify dependencies, as well. If you just want to verify dependencies, you can also add the `--skip-source` flag. Attempting to use the `--skip-source` flag without including the `--verify-deps` flag results in an error because there is essentially nothing to verify.

Running `sui client verify-source --skip-source --verify-deps` does not publish the package, but performs the same dependency verification as `sui client publish`. You could use this command to check dependency verification before attempting to publish, as described in the [previous section](#publish-packages).

The `sui client verify-source` command expects package on-chain addresses to be set in the package manifest. There should not be any unspecified or `0x0` addresses in the package. If you want to verify a seemingly unpublished package against an on-chain address, use the `--address-override` flag to supply the on-chain address to verify against. This flag only supports packages that are truly unpublished, with all modules at address `0x0`. You receive an error if you attempt to use this flag on a published (or somehow partially published) package.

If successful, the command returns a `0` exit code and prints `Source verification succeeded!` to the console. If it fails, it returns a non-zero exit code and prints an error message to the console.

## Customize genesis

You can provide a genesis configuration file using the `--config` flag to customize the genesis process.

```shell
sui genesis --config <Path to genesis config file>
```

Example `genesis.yaml`:

```yaml
---
validator_config_info: ~
committee_size: 4
accounts:
  - gas_objects:
      - object_id: "0x33e3e1d64f76b71a80ec4f332f4d1a6742c537f2bb32473b01b1dcb1caac9427"
        gas_value: 100000
    gas_object_ranges: []
move_packages: ["<Paths to custom move packages>"]
sui_framework_lib_path: ~
move_framework_lib_path: ~

```
