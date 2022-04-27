---
title: Local REST Server & REST API Quick Start
---

Welcome to the Sui REST API quick start.

This document walks you through setting up your own local Sui REST Server and using the Sui REST API to interact with a local Sui network. This guide is useful for developers interested in Sui network interactions via API. For a similar guide on Sui network interactions via CLI, refer to the [wallet](wallet.md) documentation.

Full [API documentation](https://app.swaggerhub.com/apis/MystenLabs/sui-api) can
be found on SwaggerHub.

## Local REST server setup
Follow the instructions to [install Sui binaries](install.md).

### Start local Sui Network
Follow the instructions to [create](wallet.md#genesis) and [start](wallet.md#starting-the-network) the Sui network.
The genesis process will create a `gateway.conf` configuration file, which will be used by the REST server.

### Start local REST server

Use the following command to start a local server:
```shell
$ rest_server
```
You will see output resembling:
```
INFO listening, local_addr: 127.0.0.1:5001
```

NOTE: For additional logs, set `RUST_LOG=debug` before invoking `rest_server`

Export a local user variable to store the hardcoded hostname + port that the local REST server starts with to be used when issuing the curl commands that follow.
```shell
$ export SUI_GATEWAY_HOST=http://127.0.0.1:5001
```

## Sui REST API

In the following sections we will show how to use Sui's REST API using
the `curl` command. You can also access it using
[Postman](https://www.postman.com/) which we describe in the [Postman
setup section](#postman-setup).

## Sui endpoints

### GET /docs

Retrieve OpenAPI documentation:

```shell
$ curl --location --request GET $SUI_GATEWAY_HOST/docs | json_pp
```

### POST /api/sync_account_state

Synchronize client state with validators:

```shell
$ curl --location --request POST $SUI_GATEWAY_HOST/api/sync_account_state \
--header 'Content-Type: application/json' \
--data-raw '{
    "address": "{{address}}"
}'
```

You should replace `{{address}}` in the command above with an actual
address value, for example one obtained from `wallet.conf` (without quotes).

This will fetch the latest information on all objects owned by each
address that is managed by this server. This command has no output.

### GET /api/objects

Return the list of objects owned by an address:
```shell
$ curl --location --request GET $SUI_GATEWAY_HOST'/api/objects?address={{address}}' | json_pp
```

You should replace `{{address}}` in the command above with an actual
address value, you can retrieve the list of the addresses created during
genesis from `wallet.conf`. Ensure you have run [`POST
/api/sync_account_state`](#post-apisync_account_state)

The output you see should resemble the following (abbreviated to show only two objects):

```shell
{
   "objects" : [
      {
         "digest" : "0X4CjesLYhfXITheZHEasvmQXYrk92lvhUkxF7cKhvU=",
         "objectId" : "75c660b4e115aa312b56d0e46898a1673a4649f3",
         "version" : 0
      },
      {
         "digest" : "zgfZRhRIzYGbadG3vYuF2I8MPDPPoJuEfEbj4nnW9hY=",
         "objectId" : "84fb76c2ed58021ffdef956d9a6fd63852b2506d",
         "version" : 0
      },
...
}
```

### GET /object_info

Return the object information for a specified object, for example:

```shell
$ curl --location --request GET $SUI_GATEWAY_HOST'/api/object_info?objectId={{object_id}}' | json_pp
```

Replace `{{object_id}}` in the command above with an
actual object ID, for example one obtained from [`GET
/objects`](#get-objects) (without quotes).

### GET /object_schema

Return the schema for a specified object:

```shell
$ curl --location --request GET $SUI_GATEWAY_HOST'/object_schema?objectId={{object_id}}' | json_pp
```

Replace `{{object_id}}` in the command above with an
actual object ID, for example one obtained from [`GET
/objects`](#get-objects) (without quotes).

### POST /api/new_transfer
#### 1, Create a transaction to transfer an object from one address to another:
```shell
$ curl --location --request POST $SUI_GATEWAY_HOST/api/new_transfer \
--header 'Content-Type: application/json' \
--data-raw '{
    "fromAddress": "{{owner_address}}",
    "objectId": "{{coin_object_id}}",
    "toAddress": "{{to_address}}",
    "gasObjectId": "{{gas_object_id}}",
    "gasBudget" : 10000
}' | json_pp
```
A transaction data response will be returned from the gateway server.
```json
{
   "tx_bytes" : "VHJhbnNhY3Rpb25EYXRhOjoAACg4ZDZlMzM1ODIyYTA3MDk2MWUxZjU0ODk1OTEyZjViMzU0YjU1MzdkhPt2wu1YAh/975Vtmm/WOFKyUG0AAAAAAAAAACx6Z2ZaUmhSSXpZR2JhZEczdll1RjJJOE1QRFBQb0p1RWZFYmo0bm5XOWhZPSg4ZDZlMzM1ODIyYTA3MDk2MWUxZjU0ODk1OTEyZjViMzU0YjU1MzdkdcZgtOEVqjErVtDkaJihZzpGSfMAAAAAAAAAACwwWDRDamVzTFloZlhJVGhlWkhFYXN2bVFYWXJrOTJsdmhVa3hGN2NLaHZVPRAnAAAAAAAA"
}
```
#### 2, Sign the transaction using the Sui signtool
```shell
$ sui signtool --address <owner_address> --data <tx_bytes>
```
The signing tool will create and print out the signature and public key information.
You will see output resembling:
```shell
2022-04-04T21:42:44.915471Z  INFO sui::sui_commands: Data to sign : VHJhbnNhY3Rpb25EYXRhOjoAACg4ZDZlMzM1ODIyYTA3MDk2MWUxZjU0ODk1OTEyZjViMzU0YjU1MzdkhPt2wu1YAh/975Vtmm/WOFKyUG0AAAAAAAAAACx6Z2ZaUmhSSXpZR2JhZEczdll1RjJJOE1QRFBQb0p1RWZFYmo0bm5XOWhZPSg4ZDZlMzM1ODIyYTA3MDk2MWUxZjU0ODk1OTEyZjViMzU0YjU1MzdkdcZgtOEVqjErVtDkaJihZzpGSfMAAAAAAAAAACwwWDRDamVzTFloZlhJVGhlWkhFYXN2bVFYWXJrOTJsdmhVa3hGN2NLaHZVPRAnAAAAAAAA
2022-04-04T21:42:44.915626Z  INFO sui::sui_commands: Address : 8D6E335822A070961E1F54895912F5B354B5537D
2022-04-04T21:42:44.915915Z  INFO sui::sui_commands: Public Key Base64: jAs+VmaKuEf4fpLNiLIHknEpft3zmy9b1v3AOYn/v4g=
2022-04-04T21:42:44.915919Z  INFO sui::sui_commands: Signature : FULrl7iI1aQnb5OgCksYWJw6/Fv44wZZfeMXXN7CZJyooR2+Iu0WIVWUO61+r47aEidLeIewV8iLXFW7GPoYAw==
```

#### 3, Execute the transaction using the transaction data, signature and public key.
```shell
$ curl --location --request POST $SUI_GATEWAY_HOST/api/execute_transaction \
--header 'Content-Type: application/json' \
--data-raw '{
    "tx_bytes": "{{tx_bytes}}",
    "signature": "{{signature}}",
    "pub_key": "{{public_key_base64}}"
}' | json_pp
```

Native transfer by `POST /new_transfer` is supported for coin
objects only (including gas objects). Refer to
[transactions](transactions.md#native-transaction) documentation for
more information about a native transfer. Non-coin objects cannot be
transferred natively and require a [Move call](#post-apimove_call).

You should replace `{{owner_address}}` and `{{to_address}}` in the
command above with an actual address values, for example one obtained
from `wallet.conf`. You should also replace
`{{coin_object_id}}` and `{{gas_object_id}}` in the command above with
an actual object ID, for example one obtained from `objectId` in the output
of [`GET /objects`](#get-apiobjects). You can see that all gas objects generated
during genesis are of `Coin/SUI` type). For this call to work, objects
represented by both `{{coin_object_id}}` and `{{gas_object_id}}` must
be owned by the address represented by `{{owner_address}}`.


### POST /api/move_call

#### 1, Execute a Move call transaction by calling the specified function in
the module of a given package (smart contracts in Sui are written in
the [Move](move.md#move) language):

```shell
$ curl --location --request POST $SUI_GATEWAY_HOST/api/move_call \
--header 'Content-Type: application/json' \
--data-raw '{
    "sender": "{{owner_address}}",
    "packageObjectId": "0x2",
    "module": "GAS",
    "function": "transfer",
    "args": [
        "0x{{coin_object_id}}",
        "0x{{to_address}}"
    ],
    "gasObjectId": "{{gas_object_id}}",
    "gasBudget": 2000
}' | json_pp
```

#### 2, Sign the transaction
Follow the instructions to [sign the transaction](rest-api.md#2-sign-the-transaction-using-the-sui-signtool).

#### 3, Execute the transaction
Follow the instructions to [execute the transaction](rest-api.md#3-execute-the-transaction-using-the-transaction-data-signature-and-public-key).

Arguments are passed in, and type will be inferred from function
signature.  Gas usage is capped by the gas_budget. The `transfer`
function is described in more detail in
the [Sui Wallet](wallet.md#calling-move-code) documentation.

Calling the `transfer` function in the `GAS` module serves the same
purpose as the native coin transfer ([`POST
/api/transfer`](#post-apinew_transfer)), and is mostly used for illustration
purposes as native transfer is more efficient when it's applicable
(i.e., we are transferring coins rather than non-coin
objects). Consequently, you should fill out argument placeholders
(`{{owner_address}}`, `{{coin_object_id}`, etc.) the same way you
would for [`POST /api/transfer`](#post-apinew_transfer) - please not additional
`0x` prepended to function arguments.

NOTE: A Publish endpoint is in the works, but for now the only way to add a new module is to have it included as part of genesis. To do this, add your Move module to `sui_programmability/framework/sources` before you hit the genesis endpoint. Once you have done this you will be able to use `"packageObjectId": "0x2"` in the call endpoint to find your Move module.

To learn more about what `args` are accepted in a Move call, refer to the [SuiJSON](sui-json.md) documentation.

### POST /api/publish

Publish Move module.

```shell
$ curl --location --request POST $SUI_GATEWAY_HOST/publish \
--header 'Content-Type: application/json' \
--data-raw '{
    "sender": "{{owner_address}}",
    "compiledModules": {{vector_of_compiled_modules}},
    "gasObjectId": "{{gas_object_id}}",
    "gasBudget": 10000
}' | json_pp
```

This endpoint will perform proper verification and linking to make
sure the package is valid. If some modules have [initializers](move.md#module-initializers), these initializers
will also be executed in Move (which means new Move objects can be created in
the process of publishing a Move package). Gas budget is required because of the
need to execute module initializers.

You should replace `{{owner_address}}` in the
command above with an actual address values, for example one obtained
from `wallet.conf`. You should also replace `{{gas_object_id}}` in the command above with
an actual object ID, for example one obtained from `objectId` in the output
of [`GET /objects`](#get-apiobjects). You can see that all gas objects generated
during genesis are of `Coin/SUI` type). For this call to work, the object
represented by `{{gas_object_id}}` must be owned by the address represented by
`{{owner_address}}`.

To publish a Move module, you also need `{{vector_of_compiled_modules}}`. To generate the value of this field, use the `sui-move` command. The `sui-move` command supports printing the bytecodes as base64 with the following option

```
$ sui-move --path <move-module-path> build --dump-bytecode-as-base64
```

Assuming that the location of the package's sources is in the `PATH_TO_PACKAGE` environment variable an example command would resemble the following

```
$ sui-move --path $PATH_TO_PACKAGE/my_move_package build --dump-bytecode-as-base64

["oRzrCwUAAAAJAQAIAggUAxw3BFMKBV1yB88BdAjDAigK6wIFDPACQgAAAQEBAgEDAAACAAEEDAEAAQEBDAEAAQMDAgAABQABAAAGAgEAAAcDBAAACAUBAAEFBwEBAAEKCQoBAgMLCwwAAgwNAQEIAQcODwEAAQgQAQEABAYFBgcICAYJBgMHCwEBCAALAgEIAAcIAwABBwgDAwcLAQEIAAMHCAMBCwIBCAADCwEBCAAFBwgDAQgAAgsCAQkABwsBAQkAAQsBAQgAAgkABwgDAQsBAQkAAQYIAwEFAgkABQMDBwsBAQkABwgDAQsCAQkAAgsBAQkABQdNQU5BR0VEBENvaW4IVHJhbnNmZXIJVHhDb250ZXh0C1RyZWFzdXJ5Q2FwBGJ1cm4EaW5pdARtaW50DHRyYW5zZmVyX2NhcAtkdW1teV9maWVsZA9jcmVhdGVfY3VycmVuY3kGc2VuZGVyCHRyYW5zZmVyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgACAQkBAAEAAAEECwELADgAAgEAAAAICwkSAAoAOAEMAQsBCwAuEQY4AgICAQAAAQULAQsACwI4AwIDAQAAAQQLAAsBOAQCAA==", "oRzrCwUAAAALAQAOAg4kAzJZBIsBHAWnAasBB9IC6QEIuwQoBuMECgrtBB0MigWzAQ29BgYAAAABAQIBAwEEAQUBBgAAAgAABwgAAgIMAQABBAQCAAEBAgAGBgIAAxAEAAISDAEAAQAIAAEAAAkCAwAACgQFAAALBgcAAAwEBQAADQQFAAIVCgUBAAIICwMBAAIWDQ4BAAIXERIBAgYYAhMAAhkCDgEABRoVAwEIAhsWAwEAAgsXDgEAAg0YBQEABgkHCQgMCA8JCQsMCw8MFAYPBgwNDA0PDgkPCQMHCAELAgEIAAcIBQILAgEIAwsCAQgEAQcIBQABBggBAQMEBwgBCwIBCAMLAgEIBAcIBQELAgEIAAMLAgEIBAMLAgEIAwEIAAEGCwIBCQACCwIBCQAHCwcBCQABCAMDBwsCAQkAAwcIBQELAgEJAAEIBAELBwEIAAIJAAcIBQELBwEJAAEIBgEIAQEJAAIHCwIBCQALAgEJAAMDBwsHAQkABwgFAQYLBwEJAAZCQVNLRVQHTUFOQUdFRARDb2luAklEA1NVSQhUcmFuc2ZlcglUeENvbnRleHQHUmVzZXJ2ZQRidXJuBGluaXQObWFuYWdlZF9zdXBwbHkEbWludApzdWlfc3VwcGx5DHRvdGFsX3N1cHBseQtkdW1teV9maWVsZAJpZAtWZXJzaW9uZWRJRAx0cmVhc3VyeV9jYXALVHJlYXN1cnlDYXADc3VpB21hbmFnZWQFdmFsdWUId2l0aGRyYXcPY3JlYXRlX2N1cnJlbmN5Bm5ld19pZAR6ZXJvDHNoYXJlX29iamVjdARqb2luAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgMIAAAAAAAAAAAAAgEOAQECBA8IBhELBwEIABMLAgEIAxQLAgEIBAABAAAIFg4BOAAMBAsBCgAPADgBCgAPAQoECgI4AgwFCwAPAgsECwI4AwwDCwULAwIBAAAAEA8JEgAKADgEDAEKABEKCwEKADgFCwA4BhIBOAcCAgEAAAMECwAQAjgIAgMBAAAFHA4BOAkMBAoEDgI4CCEDDgsAAQsDAQcAJwoADwELATgKCgAPAgsCOAsLBAsADwALAzgMAgQBAAADBAsAEAE4CQIFAQAAAwQLABAAOA0CAQEBAgEDAA=="]
Build Successful
```

Copy the outputting base64 representation of the compiled Move module into the
REST publish endpoint.

#### 2, Sign the transaction
Follow the instructions to [sign the transaction](rest-api.md#2-sign-the-transaction-using-the-sui-signtool).

#### 3, Execute the transaction
Follow the instructions to [execute the transaction](rest-api.md#3-execute-the-transaction-using-the-transaction-data-signature-and-public-key).

Below you can see a truncated sample output of [POST /publish](#post-publish). One of the results of executing this command is generation of a package object representing the published Move code. An ID of the package object can be used as an argument for subsequent Move calls to functions defined in this package.

```
{
    "package": [
            "13e3ec7279060663e1bbc45aaf5859113fc164d2",
    ...
}
```

## Postman setup
The recommended way to test the Sui REST API is to use Postman.

Postman is an API platform for building and using APIs. Postman provides an alternative solution to accessing APIs over issuing `curl` commands in a terminal. You can use variables rather than copy-pasting addresses and object IDs for each call in a terminal. We have provided a sample Postman runbook for you to use.

[![Run in Postman](https://run.pstmn.io/button.svg)](https://app.getpostman.com/run-collection/fcfc1dac0f8073f92734?action=collection%2Fimport)

Click **Run in Postman**, log in, import the collection into your workspace, and fire calls at the network.

Use:
* After clicking **Run in Postman**, create an account with Postman if you don't already have one. After doing so, you will be able to import the sample collection into your workspace.
* Running against localhost requires you to use the desktop version of Postman which is free to download.
* Our sample Postman runbook has test scripts set up to automatically strip the JSON response and set variables for future calls. (i.e. owner, gas_object_id, coin, object, to)
* Refer to [Run in Postman](https://learning.postman.com/docs/publishing-your-api/run-in-postman/introduction-run-button/) documentation for more information on running the app.
* Refer to [Postman](https://learning.postman.com/docs/getting-started/introduction/) documentation for more general usage information.

## Connect to remote REST server

Coming soon - alternative ways of working with Sui's REST API. Connect to Sui devnet, testnet, or mainnet!
