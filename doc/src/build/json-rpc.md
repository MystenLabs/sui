---
title: Local RPC Server & JSON-RPC API Quick Start
---

Welcome to the Sui RPC server quick start.

This document walks you through setting up your own local Sui RPC Server and using the Sui JSON-RPC API to interact with a local Sui network. This guide is useful for developers interested in Sui network interactions via API. For a similar guide on Sui network interactions via CLI, refer to the [Sui CLI client](cli-client.md) documentation.


## Local RPC server setup
Follow the instructions to [install Sui binaries](install.md).

### Start local Sui network
Follow the instructions to [create](cli-client.md#genesis) and [start](cli-client.md#starting-the-network) the Sui network.
The genesis process will create a `gateway.conf` configuration file that will be used by the RPC server.

### Start local RPC server

Use the following command to start a local server:
```shell
$ rpc-server
```
You will see output resembling:
```
2022-04-25T11:06:40.147259Z  INFO rpc_server: Gateway config file path: ".sui/sui_config/gateway.conf"
2022-04-25T11:06:40.147277Z  INFO rpc_server: AccessControl { allowed_hosts: Any, allowed_origins: None, allowed_headers: Any, continue_on_invalid_cors: false }
2022-04-25T11:06:40.163568Z  INFO rpc_server: Available JSON-RPC methods : ["sui_moveCall", "sui_getTransaction", "sui_getObjectTypedInfo", "sui_getTotalTransactionNumber", "sui_getOwnedObjects", "sui_getObjectInfoRaw", "sui_transferObject", "sui_executeTransaction", "sui_mergeCoins", "sui_getRecentTransactions", "sui_getTransactionsInRange", "rpc.discover", "sui_splitCoin", "sui_publish", "sui_syncAccountState"]
2022-04-25T11:06:40.163590Z  INFO rpc_server: Sui RPC Gateway listening on local_addr:127.0.0.1:5001
```

> **Note:** For additional logs, set `RUST_LOG=debug` before invoking `rpc-server`.

Export a local user variable to store the hardcoded hostname + port that the local RPC server starts with to be used when issuing the `curl` commands that follow.
```shell
export SUI_RPC_HOST=http://127.0.0.1:5001
```

## Sui JSON-RPC API

In the following sections we will show how to use Sui's JSON-RPC API with
the `curl` command.

## Sui JSON-RPC methods

### rpc.discover

Sui RPC server supports OpenRPC’s [service discovery method](https://spec.open-rpc.org/#service-discovery-method).
A `rpc.discover` method is added to provide documentation describing our JSON-RPC APIs service.

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"rpc.discover","id":1}'
```

You can see an example of the discovery service in the [OpenRPC Playground](https://playground.open-rpc.org/?schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/189d61df846f7c3676c1215cc41fb970ee9e22b5/sui/open_rpc/spec/openrpc.json).

### sui_syncAccountState

Synchronize client state with validators with the following command,
replacing `{{address}}` with an actual address value, for example one obtained from `client.yaml`:

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"sui_syncAccountState", "params":["{{address}}"], "id":1}'
```

This will fetch the latest information on all objects owned by each
address that is managed by this server. This command has no output.

### sui_getOwnedObjects

Return the list of objects owned by an address:
```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"sui_getOwnedObjects", "params":["{{address}}"], "id":1}' | json_pp
```

You should replace `{{address}}` in the command above with an actual
address value, you can retrieve the list of the addresses created during
genesis from `client.yaml`. Ensure you have run [`sui_syncAccountState`](#sui_syncaccountstate)

The output you see should resemble the following (abbreviated to show only two objects):

```shell
{
   "id" : 1,
   "jsonrpc" : "2.0",
   "result" : {
      "objects" : [
         {
            "digest" : "zpa45U9ANfA9A6iS01NvAoVH0RbYB6a5rjhgh2Hb/GE=",
            "objectId" : "0x17b348903b0cfb75fc9ab5426bb69d83d1e756a5",
            "version" : 1
         },
         {
            "digest" : "8SPi0h6xVMVNBvGzzF4RfuOoaXISdtiB5aT7+BYDbxg=",
            "objectId" : "0x7599d8ea1de4c9616d077f16ca0eb38cdecacc07",
            "version" : 1
         },
         ...
      ]
   }
}

```

### GET sui_getObjectInfoRaw

Return the object information for a specified object, for example:

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"sui_getObjectInfoRaw", "params":["{{object_id}}"], "id":1}' | json_pp
```

Replace `{{object_id}}` in the command above with an
actual object ID, for example one obtained from [`sui_getOwnedObjects`](#sui_getownedobjects) (without quotes).

### sui_transferObject
#### 1, Create a transaction to transfer a Sui coin from one address to another:
```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0",
              "method":"sui_transferObject",
              "params":["{{owner_address}}",
                        "{{object_id}}",
                        "{{gas_object_id}}",
                        {{gas_budget}},
                        "{{to_address}}"],
              "id":1}' | json_pp
```
A transaction data response will be returned from the gateway server.
```json
{
  "id" : 1,
  "jsonrpc" : "2.0",
  "result" : {
    "tx_bytes" : "VHJhbnNhY3Rpb25EYXRhOjoAAFHe8jecgzoGWyGlZ1sJ2KBFN8aZF7NIkDsM+3X8mrVCa7adg9HnVqUBAAAAAAAAACDOlrjlT0A18D0DqJLTU28ChUfRFtgHprmuOGCHYdv8YVHe8jecgzoGWyGlZ1sJ2KBFN8aZdZnY6h3kyWFtB38Wyg6zjN7KzAcBAAAAAAAAACDxI+LSHrFUxU0G8bPMXhF+46hpchJ22IHlpPv4FgNvGOgDAAAAAAAA"
  }
}

```
#### 2, Sign the transaction using the Sui signtool
```shell
sui keytool sign --address <owner_address> --data <tx_bytes>
```
The signing tool will create and print out the signature and public key information.
You will see output resembling:
```shell
2022-04-25T18:50:06.031722Z  INFO sui::sui_commands: Data to sign : VHJhbnNhY3Rpb25EYXRhOjoAAFHe8jecgzoGWyGlZ1sJ2KBFN8aZF7NIkDsM+3X8mrVCa7adg9HnVqUBAAAAAAAAACDOlrjlT0A18D0DqJLTU28ChUfRFtgHprmuOGCHYdv8YVHe8jecgzoGWyGlZ1sJ2KBFN8aZdZnY6h3kyWFtB38Wyg6zjN7KzAcBAAAAAAAAACDxI+LSHrFUxU0G8bPMXhF+46hpchJ22IHlpPv4FgNvGOgDAAAAAAAA
2022-04-25T18:50:06.031765Z  INFO sui::sui_commands: Address : 0x51def2379c833a065b21a5675b09d8a04537c699
2022-04-25T18:50:06.031911Z  INFO sui::sui_commands: Public Key Base64: H82FDLUZN1u0+6UdZilxu9HDT5rPd3khKo2UJoCPJFo=
2022-04-25T18:50:06.031925Z  INFO sui::sui_commands: Signature : 6vc+ku0RsMKdky8DRfoy/hw6eCQ3YsadH6rZ9WUCwGTAumuWER3TOJRw7u7F4QaHkqUsIPfJN9GRraSX+N8ADQ==
```

#### 3, Execute the transaction using the transaction data, signature and public key.
```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0",
              "method":"sui_executeTransaction",
              "params":[{
                  "tx_bytes" : "{{tx_bytes}}",
                  "signature" : "{{signature}}",
                  "pub_key" : "{{pub_key}}"}],
              "id":1}' | json_pp
```

Native transfer by `sui_transferObject` is supported for any object that allows for public transfers. Refer to
[transactions](transactions.md#native-transaction) documentation for
more information about a native transfer. Some objects cannot be
transferred natively and require a [Move call](#sui_movecall).

You should replace `{{owner_address}}` and `{{to_address}}` in the
command above with an actual address values, for example one obtained
from `client.yaml`. You should also replace
`{{object_id}}` and `{{gas_object_id}}` in the command above with
an actual object ID, for example one obtained from `objectId` in the output
of [`sui_getOwnedObjects`](#sui_getownedobjects). You can see that all gas objects generated
during genesis are of `Coin/SUI` type). For this call to work, objects
represented by both `{{coin_object_id}}` and `{{gas_object_id}}` must
be owned by the address represented by `{{owner_address}}`.


### sui_moveCall

#### 1, Execute a Move call transaction by calling the specified function in
the module of a given package (smart contracts in Sui are written in
the [Move](move.md#move) language):

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc": "2.0",
              "method": "sui_moveCall",
              "params": [
                  "{{owner_address}}",
                  "0x2",
                  "coin",
                  "transfer",
                  ["0x2::sui::sui"],
                  ["{{object_id}}", "{{recipient_address}}"],
                  "{{gas_object_id}}",
                  2000
              ],
              "id": 1 }' | json_pp
```

#### 2, Sign the transaction
Follow the instructions to [sign the transaction](#2-sign-the-transaction-using-the-sui-signtool).

#### 3, Execute the transaction
Follow the instructions to [execute the transaction](#3-execute-the-transaction-using-the-transaction-data-signature-and-public-key).

Arguments are passed in, and type will be inferred from function
signature.  Gas usage is capped by the gas_budget. The `transfer`
function is described in more detail in
the [Sui CLI client](cli-client.md#calling-move-code) documentation.

Calling the `transfer` function in the `Coin` module serves the same
purpose as the native transfer ([`sui_transferObject`](#sui_TransferObject)), and is mostly used for illustration
purposes as native transfer is more efficient when it's applicable
(i.e., we are simply transferring objects with no additional Move logic). Consequently, you should fill out argument placeholders
(`{{owner_address}}`, `{{object_id}`, etc.) the same way you
would for [`sui_transferObject`](#sui_TransferObject) - please note additional
`0x` prepended to function arguments.

To learn more about what `args` are accepted in a Move call, refer to the [SuiJSON](sui-json.md) documentation.

### sui_publish

Publish Move module.

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0",
              "method":"sui_publish",
              "params":[ "{{owner_address}}",
                         {{vector_of_compiled_modules}},
                         "{{gas_object_id}}",
                         10000],
              "id":1}' | json_pp
```

This endpoint will perform proper verification and linking to make
sure the package is valid. If some modules have [initializers](move.md#module-initializers), these initializers
will also be executed in Move (which means new Move objects can be created in
the process of publishing a Move package). Gas budget is required because of the
need to execute module initializers.

You should replace `{{owner_address}}` in the
command above with an actual address values, for example one obtained
from `client.yaml`. You should also replace `{{gas_object_id}}` in the command above with
an actual object ID, for example one obtained from `objectId` in the output
of [`sui_getOwnedObjects`](#sui_getownedobjects). You can see that all gas objects generated
during genesis are of `Coin/SUI` type). For this call to work, the object
represented by `{{gas_object_id}}` must be owned by the address represented by
`{{owner_address}}`.

To publish a Move module, you also need `{{vector_of_compiled_modules}}`. To generate the value of this field, use the `sui-move` command. The `sui-move` command supports printing the bytecodes as base64 with the following option

```
sui move --path <move-module-path> build --dump-bytecode-as-base64
```

Assuming that the location of the package's sources is in the `PATH_TO_PACKAGE` environment variable an example command would resemble the following

```
sui move --path $PATH_TO_PACKAGE/my_move_package build --dump-bytecode-as-base64

["oRzrCwUAAAAJAQAIAggUAxw3BFMKBV1yB88BdAjDAigK6wIFDPACQgAAAQEBAgEDAAACAAEEDAEAAQEBDAEAAQMDAgAABQABAAAGAgEAAAcDBAAACAUBAAEFBwEBAAEKCQoBAgMLCwwAAgwNAQEIAQcODwEAAQgQAQEABAYFBgcICAYJBgMHCwEBCAALAgEIAAcIAwABBwgDAwcLAQEIAAMHCAMBCwIBCAADCwEBCAAFBwgDAQgAAgsCAQkABwsBAQkAAQsBAQgAAgkABwgDAQsBAQkAAQYIAwEFAgkABQMDBwsBAQkABwgDAQsCAQkAAgsBAQkABQdNQU5BR0VEBENvaW4IVHJhbnNmZXIJVHhDb250ZXh0C1RyZWFzdXJ5Q2FwBGJ1cm4EaW5pdARtaW50DHRyYW5zZmVyX2NhcAtkdW1teV9maWVsZA9jcmVhdGVfY3VycmVuY3kGc2VuZGVyCHRyYW5zZmVyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgACAQkBAAEAAAEECwELADgAAgEAAAAICwkSAAoAOAEMAQsBCwAuEQY4AgICAQAAAQULAQsACwI4AwIDAQAAAQQLAAsBOAQCAA==", "oRzrCwUAAAALAQAOAg4kAzJZBIsBHAWnAasBB9IC6QEIuwQoBuMECgrtBB0MigWzAQ29BgYAAAABAQIBAwEEAQUBBgAAAgAABwgAAgIMAQABBAQCAAEBAgAGBgIAAxAEAAISDAEAAQAIAAEAAAkCAwAACgQFAAALBgcAAAwEBQAADQQFAAIVCgUBAAIICwMBAAIWDQ4BAAIXERIBAgYYAhMAAhkCDgEABRoVAwEIAhsWAwEAAgsXDgEAAg0YBQEABgkHCQgMCA8JCQsMCw8MFAYPBgwNDA0PDgkPCQMHCAELAgEIAAcIBQILAgEIAwsCAQgEAQcIBQABBggBAQMEBwgBCwIBCAMLAgEIBAcIBQELAgEIAAMLAgEIBAMLAgEIAwEIAAEGCwIBCQACCwIBCQAHCwcBCQABCAMDBwsCAQkAAwcIBQELAgEJAAEIBAELBwEIAAIJAAcIBQELBwEJAAEIBgEIAQEJAAIHCwIBCQALAgEJAAMDBwsHAQkABwgFAQYLBwEJAAZCQVNLRVQHTUFOQUdFRARDb2luAklEA1NVSQhUcmFuc2ZlcglUeENvbnRleHQHUmVzZXJ2ZQRidXJuBGluaXQObWFuYWdlZF9zdXBwbHkEbWludApzdWlfc3VwcGx5DHRvdGFsX3N1cHBseQtkdW1teV9maWVsZAJpZAtWZXJzaW9uZWRJRAx0cmVhc3VyeV9jYXALVHJlYXN1cnlDYXADc3VpB21hbmFnZWQFdmFsdWUId2l0aGRyYXcPY3JlYXRlX2N1cnJlbmN5Bm5ld19pZAR6ZXJvDHNoYXJlX29iamVjdARqb2luAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgMIAAAAAAAAAAAAAgEOAQECBA8IBhELBwEIABMLAgEIAxQLAgEIBAABAAAIFg4BOAAMBAsBCgAPADgBCgAPAQoECgI4AgwFCwAPAgsECwI4AwwDCwULAwIBAAAAEA8JEgAKADgEDAEKABEKCwEKADgFCwA4BhIBOAcCAgEAAAMECwAQAjgIAgMBAAAFHA4BOAkMBAoEDgI4CCEDDgsAAQsDAQcAJwoADwELATgKCgAPAgsCOAsLBAsADwALAzgMAgQBAAADBAsAEAE4CQIFAQAAAwQLABAAOA0CAQEBAgEDAA=="]
Build Successful
```

Copy the outputting base64 representation of the compiled Move module into the
REST publish endpoint.

#### 2, Sign the transaction
Follow the instructions to [sign the transaction](#2-sign-the-transaction-using-the-sui-signtool).

#### 3, Execute the transaction
Follow the instructions to [execute the transaction](#3-execute-the-transaction-using-the-transaction-data-signature-and-public-key).

Below you can see a truncated sample output of [sui_publish](#sui_publish). One of the results of executing this command is generation of a package object representing the published Move code. An ID of the package object can be used as an argument for subsequent Move calls to functions defined in this package.

```
{
    "package": [
            "0x13e3ec7279060663e1bbc45aaf5859113fc164d2",
    ...
}
```

## Connect to remote JSON-RPC server

Coming soon - alternative ways of working with Sui's JSON-RPC API. Connect to Sui devnet, testnet, or mainnet!
