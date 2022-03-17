---
title: Local REST Server & REST API Quick Start
---

Welcome to the Sui REST API quick start. 

This document will walk you through setting up your own local Sui REST Server and using the Sui REST API to interact with a local Sui network. This guide is useful for users that are interested in Sui network interactions via API. For a similar guide on Sui network interactions via CLI, refer to [wallet](wallet.md) documentation.  

Full [API documentation](https://app.swaggerhub.com/apis/MystenLabs/sui-api) can
be found on SwaggerHub.

## Local Rest Server Setup

## Setup

Follow the instructions to [install Sui binaries](install.md).

### Start local REST Server

Use the following command to start a local server
```shell
$ rest_server

INFO listening, local_addr: 127.0.0.1:5000
```
NOTE: For additional logs, set `RUST_LOG=debug` before invoking `./rest_server`

Export a local user variable to store the harcoded hostname + port that the local rest server starts with. This will be useful when issuing the curl commands below.
```shell
$ export SUI_GATEWAY_HOST=http://127.0.0.1:5000
```

To initialize and start the network, you need to invoke the /sui/genesis and /sui/start endpoint as mentioned below.

## Sui REST API

[![Run in Postman](https://run.pstmn.io/button.svg)](https://app.getpostman.com/run-collection/fcfc1dac0f8073f92734?action=collection%2Fimport)

The recomended way to test the Sui REST API is to use Postman. 

Postman is an API paltform for building and using APIs. Postman provides an alternative solution to accessing APIs over issuing curl commands in a terminal. You can use variables rather than copy-pasting addresses & object ids for each call in a terminal. We have provided a sample Postman runbook for you to use. Click `Run in Postman`, login and import the collection into your workspace and fire calls at the network.

Note:
- After clicking the button above, create an account with Postman if you don't already have one. After doing so you will be able to import the sample collection into your workspace.
- Running against localhost requires you to use the desktop version of Postman which is free to download.
- Our sample Postman runbook has test scripts setup to automatically strip the JSON response and set variables for future calls. (i.e. owner, gas_object_id, coin, object, to)
- Refer to [Run in Postman](https://learning.postman.com/docs/publishing-your-api/run-in-postman/introduction-run-button/) documentation, for more information on using the button above.
- Refer to [Postman](https://learning.postman.com/docs/getting-started/introduction/) documentation for more general usage information.

### Sui Network Endpoints

#### POST /sui/genesis

The `genesis` command creates four authorities and five user accounts
each with five gas objects. These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
such as object transfers or smart contract (Move) calls.

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/genesis | json_pp
```

#### POST /sui/start

This will start the Sui network with the genesis configuration specified. 

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/start | json_pp
```

#### POST /sui/stop

This will kill the authorities and all of the data stored in the network. Use
this if you want to reset the state of the network without having to kill the 
rest server.

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sui/stop
```

### Sui Endpoints

#### GET /docs

Retrieve OpenAPI documentation.

```shell
curl --location --request GET $SUI_GATEWAY_HOST/docs | json_pp
```

#### GET /addresses

Retrieve all managed addresses for this client.

```shell
curl --location --request GET $SUI_GATEWAY_HOST/addresses | json_pp
```

#### GET /objects

Returns the list of objects owned by an address.

```shell
curl --location --request GET $SUI_GATEWAY_HOST'/objects?address={{address}}' | json_pp
```

#### GET /object_info

Returns the object information for a specified object.

```shell
curl --location --request GET $SUI_GATEWAY_HOST'/object_info?objectId={{object_id}}' | json_pp
```

#### GET /object_schema

Returns the schema for a specified object.

```shell
curl --location --request GET $SUI_GATEWAY_HOST'/object_schema?objectId={{object_id}}' | json_pp
```

#### POST /transfer

Transfer object from one address to another. Gas will be paid using the gas
provided in the request. This will be done through a native transfer
transaction that does not require Move VM executions, hence is much cheaper.

Refer to [transactions](transactions.md#native-transaction) documentation for more information about a native transfer.

```shell
curl --location --request POST $SUI_GATEWAY_HOST/transfer \
--header 'Content-Type: application/json' \
--data-raw '{
    "fromAddress": "{{owner_address}}",
    "objectId": "{{coin}}",
    "toAddress": "{{to_address}}",
    "gasObjectId": "{{gas_object_id}}"
}' | json_pp
```
Notes:
- Non-coin objects cannot be transferred natively and will require a Move call. Refer to [Move](move.md#move-structs) documentation to learn more about coin objects.

#### POST /call

Execute a Move call transaction by calling the specified function in the
module of the given package. Arguments are passed in and type will be
inferred from function signature. Gas usage is capped by the gas_budget.

```shell
curl --location --request POST $SUI_GATEWAY_HOST/call \
--header 'Content-Type: application/json' \
--data-raw '{
    "sender": "{{owner_address}}",
    "packageObjectId": "0x2",
    "module": "ObjectBasics",
    "function": "create",
    "args": [
        100,
        "0x{{owner_address}}"
    ],
    "gasObjectId": "{{gas_object_id}}",
    "gasBudget": 2000
}' | json_pp
```
Notes:
- A Publish endpoint is in the works, but for now the only way to add a new module is to have it included as part of genesis. To do this add your Move module to  `sui_programmability/framework/sources` before you hit the genesis endpoint. Once you have done this you will be able to use `"packageObjectId": "0x2"` in the call endpoint to find your Move module.
- To learn more about what `args` are accepted in a Move call, refer to [sui-json](sui-json.md) documentation for further information.

#### POST /sync

Synchronize client state with authorities. This will fetch the latest information
on all objects owned by each address that is managed by this server.

```shell
curl --location --request POST $SUI_GATEWAY_HOST/sync \
--header 'Content-Type: application/json' \
--data-raw '{
    "address": "{{address}}"
}' | json_pp
```


## Connect to Remote REST Server

Coming soon - alternative ways of working with Sui's REST API. Connect to Sui devnet, testnet, or mainnet! 