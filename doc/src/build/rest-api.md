---
title: Local REST Server & REST API Quick Start
---

Welcome to the Sui REST API quick start. 

This document walks you through setting up your own local Sui REST Server and using the Sui REST API to interact with a local Sui network. This guide is useful for developers interested in Sui network interactions via API. For a similar guide on Sui network interactions via CLI, refer to the [wallet](wallet.md) documentation.  

Full [API documentation](https://app.swaggerhub.com/apis/MystenLabs/sui-api) can
be found on SwaggerHub.

## Local REST server setup

Follow the instructions to [install Sui binaries](install.md).

### Start local REST server

Use the following command to start a local server:
```shell
rest_server
```
You will see output resembling:
```
INFO listening, local_addr: 127.0.0.1:5000
```

NOTE: For additional logs, set `RUST_LOG=debug` before invoking `rest_server`

Export a local user variable to store the hardcoded hostname + port that the local REST server starts with to be used when issuing the curl commands that follow.
```shell
export SUI_GATEWAY_HOST=http://127.0.0.1:5000
```

To initialize and start the network, you need to invoke the /sui/genesis and /sui/start endpoints as mentioned below.

## Sui REST API

In the following sections we will show how to use Sui's REST API using
the `curl` command. You can also access it using
[Postman](https://www.postman.com/) which we describe in the [Postman
setup section](#postman-setup).

## Sui network endpoints

### POST /sui/genesis

The `genesis` command creates Sui's initial state including four authorities and five user accounts
each with five gas objects:

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/genesis | json_pp
```

These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
such as object transfers or smart contract (Move) calls.

### POST /sui/stop

The `stop` commands kills the authorities and all of the data stored in the network:

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/stop
```

Use this if you want to reset the state of the network without having to kill the 
REST server.

### POST /sui/start

The `start` command starts the Sui network with the genesis configuration specified: 

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/start | json_pp
```

## Sui endpoints

### GET /docs

Retrieve OpenAPI documentation:

```shell
curl --location --request GET $SUI_GATEWAY_HOST/docs | json_pp
```

### GET /addresses

Retrieve all managed addresses for this client:

```shell
curl --location --request GET $SUI_GATEWAY_HOST/addresses | json_pp
```

The output of this API call should include a list of five addresses
representing five user accounts created during Sui
[genesis](#post-suigenesis). Please note that actual numbers you will
see are going to be different than what is presented in this document,
as account addresses, object IDs, etc. are generated randomly during
Sui genesis. Consequently, **do not copy and paste the actual values
from this tutorial as they are unlikely to work for you verbatim.**
All that being said, part of the output should resemble the following:

```shell
{
   "addresses" : [
      "8C8280935E3BFDA6B48678A7D54E5FD8CA8A9E90",
      "0480D08F1E05D0D8783BFA8D512375DA8BAAE296",
      "084889A127487548D113CFCF4B2FAB05E572948E",
      "A177CCFA2A79ED340C7046315D70936C1BC048EF",
      "938AEEF943108FB80C183C56FC98DFB0D1D5147D"
   ]
}
```


### GET /objects

Return the list of objects owned by an address:
```shell
curl --location --request GET $SUI_GATEWAY_HOST'/objects?address={{address}}' | json_pp
```

You should replace `{{address}}` in the command above with an actual
address value, for example one obtained from [`GET
/addresses`](#get-addresses) (without quotes).

The output you see should resemble the following (abbreviated to show only two objects):

```shell
{
   "objects" : [
      {
         "objType" : "0x2::Coin::Coin<0x2::GAS::GAS>",
         "objectDigest" : "o#3eda7d747374e1cb91921b1d4bd280fa557c7da17cfee364028f07adc20a7965",
         "objectId" : "2AA11F96C434880B6D49FFEBFCA8906BA0338495",
         "version" : "SequenceNumber(0)"
      },
      {
         "objType" : "0x2::Coin::Coin<0x2::GAS::GAS>",
         "objectDigest" : "o#d78e46c4e7866f8fd87883bd48e64e53c6e1d63c3314ab1aab3af7671043e363",
         "objectId" : "5D4D8CA4F3E0882A6180E6B8D5903CAB814C2C9B",
         "version" : "SequenceNumber(0)"
      },
...
}
```

### GET /object_info

Return the object information for a specified object, for example:

```shell
curl --location --request GET $SUI_GATEWAY_HOST'/object_info?objectId={{object_id}}' | json_pp
```

Replace `{{object_id}}` in the command above with an
actual object ID, for example one obtained from [`GET
/objects`](#get-objects) (without quotes).

### GET /object_schema

Return the schema for a specified object:

```shell
curl --location --request GET $SUI_GATEWAY_HOST'/object_schema?objectId={{object_id}}' | json_pp
```

Replace `{{object_id}}` in the command above with an
actual object ID, for example one obtained from [`GET
/objects`](#get-objects) (without quotes).


### POST /transfer

Transfer an object from one address to another:

```shell
curl --location --request POST $SUI_GATEWAY_HOST/transfer \
--header 'Content-Type: application/json' \
--data-raw '{
    "fromAddress": "{{owner_address}}",
    "objectId": "{{coin_object_id}}",
    "toAddress": "{{to_address}}",
    "gasObjectId": "{{gas_object_id}}"
}' | json_pp
```

Native transfer by `POST /transfer` is supported for coin
objects only (including gas objects). Refer to
[transactions](transactions.md#native-transaction) documentation for
more information about a native transfer. Non-coin objects cannot be
transferred natively and require a [Move call](#post-call).

You should replace `{{owner_address}}` and {{to_address}}' in the
command above with an actual address values, for example one obtained
from [`GET /addresses`](#get-addresses). You should also replace
`{{coin_object_id}}` and `{{gas_object_id}}` in the command above with
an actual object ID, for example one obtained from [`GET
/objects`](#get-objects) (from `objType` in the output of [`GET
/objects`](#get-objects). You can see that all objects generated
during genesis are of `Coin/GAS` type). For this call to work, objects
represented by both `{{coin_object_id}}` and `{{gas_object_id}}` must
be owned by the address represented by `{{owner_address}}`.


#### POST /call

Execute a Move call transaction by calling the specified function in
the module of a given package (smart contracts in Sui are written in
the [Move](move.md#move) language):

```shell
curl --location --request POST $SUI_GATEWAY_HOST/call \
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

Arguments are passed in, and type will be inferred from function
signature.  Gas usage is capped by the gas_budget. The `transfer`
function is described in more detail in
the [Sui Wallet](wallet.md#calling-move-code) documentation.

Calling the `transfer` function in the `GAS` module serves the same
purpose as the native coin transfer ([`POST
/transfer`](#post-transfer)), and is mostly used for illustration
purposes as native transfer is more efficient when it's applicable
(i.e., we are transferring coins rather than non-coin
objects). Consequently, you should fill out argument placeholders
(`{{owner_address}}`, `{{coin_object_id}`, etc.) the same way you
would for [`POST /transfer`](#post-transfer) - please not additional
`0x` prepended to function arguments.

NOTE: A Publish endpoint is in the works, but for now the only way to add a new module is to have it included as part of genesis. To do this, add your Move module to  `sui_programmability/framework/sources` before you hit the genesis endpoint. Once you have done this you will be able to use `"packageObjectId": "0x2"` in the call endpoint to find your Move module.

To learn more about what `args` are accepted in a Move call, refer to the [SuiJSON](sui-json.md) documentation.

#### POST /sync

Synchronize client state with authorities:

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sync \
--header 'Content-Type: application/json' \
--data-raw '{
    "address": "{{address}}"
}' | json_pp
```

You should replace `{{address}}` in the command above with an actual
address value, for example one obtained from [`GET
/addresses`](#get-addresses) (without quotes).

This will fetch the latest information on all objects owned by each
address that is managed by this server. This command has no output.


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
