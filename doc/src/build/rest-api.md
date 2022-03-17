---
title: Local REST Server & REST API Quick Start
---

Welcome to the Sui REST API. 

This document will walk you through setting up your own local Sui REST Server and using the Sui REST API to interact with a local Sui network. This guide is useful for users that are interested in Sui network interactions via API. For a similar guide on Sui network interactions via CLI, refer to [wallet](https://github.com/MystenLabs/sui/blob/df4bbfa2d6672b884e9afc25b71d1f6243428dde/doc/src/build/wallet.md) documentation.  

Full [API documentation](https://app.swaggerhub.com/apis/MystenLabs/sui-api) can
be found on SwaggerHub.

## Local Rest Server Setup

### Installing the binaries

Sui is written in Rust and we are using Cargo to build and manage the dependencies.
As a prerequisite, you will need to [install cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) 
in order to build and install Sui on your machine.

Check out the [Sui GitHub](https://github.com/MystenLabs/sui) repository.

To install the `rest_server` binary, use the following commands.
```shell
cargo install --git ssh://git@github.com/MystenLabs/sui.git
```
or 
```shell
cargo install --path <Path to Sui project>/sui
```

This will install the `rest_server` binary in `~/.cargo/bin` directory that can be executed directly.

### Start local REST Server

Use the following command to start a local server
```shell
./rest_server
```
NOTE: For additional logs, set `RUST_LOG=debug` before invoking `./rest_server`

Export a local user variable to store the hostname + port of the local rest server. This will be useful when issuing the curl commands below.
```shell
export SUI_GATEWAY_HOST=http://127.0.0.1:5000
```

To initialize and start the network, you need to invoke the /sui/genesis and /sui/start endpoint as mentioned below.

## Connect to remote REST Server

Coming soon: Connect to Sui devnet, testnet, or mainnet

## Sui REST API

[![Run in Postman](https://run.pstmn.io/button.svg)](https://app.getpostman.com/run-collection/fcfc1dac0f8073f92734?action=collection%2Fimport)

The recomended way to test the Sui REST API is to use Postman. 

Use variables rather than copy-pasting addresses & object ids for each call. We have provided a sample Postman runbook for you to use. Import the collection into your workspace and fire calls at the network.

Note:
- Running against localhost requires you to use the desktop version of Postman.
- Sample Postman runbook has test scripts setup to automatically strip the JSON response and set variables for future calls. (i.e. owner, gas_object_id, coin, object, to)
- Refer to [Postman](https://learning.postman.com/docs/getting-started/introduction/) documentation for more information.

### Sui Network Endpoints

#### POST /sui/genesis

The `genesis` command creates four authorities and five user accounts
each with five gas objects. These are Sui [objects](https://github.com/MystenLabs/sui/blob/cef5136b9af3dfdb767d0cc77d61356f1df6ff96/doc/src/build/objects.md) used
to pay for Sui [transactions](https://github.com/MystenLabs/sui/blob/5c48a87ceee35ccbf0d6276bb3ef17bf5a0eb7d5/doc/src/build/transactions.md#native-transaction),
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

Refer to [transactions](https://github.com/MystenLabs/sui/blob/5c48a87ceee35ccbf0d6276bb3ef17bf5a0eb7d5/doc/src/build/transactions.md#native-transaction) documentation for more information about a native transfer.

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
- Non-coin objects cannot be transferred natively and will require a Move call. Refer to [Move](https://github.com/MystenLabs/sui/blob/df4bbfa2d6672b884e9afc25b71d1f6243428dde/doc/src/build/move.md#move-structs) documentation to learn more about coin objects.

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
- To learn more about what `args` are accepted in a Move call, refer to [sui-json](https://github.com/MystenLabs/sui/blob/6b6cc14f14a8cd71b87b560524373bd0faa2689c/doc/src/build/sui-json.md) documentation for further information.

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
