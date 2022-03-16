---
title: Local REST Server & REST API Quick Start
---

Welcome to the Sui REST API. 

This document will walk you through setting up your own local Sui REST Server 
and using the Sui REST API to interact with a local Sui network.

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

### Start REST Server

Use the following command to start a local server
```shell
./rest_server
```
NOTE: For additional logs, set `RUST_LOG=debug` before invoking `./rest_server`

## Sui REST APIs

### Hostname

Eventually there will be a devnet, testnet and mainnet that will be used but for
now when we refer to `HOST` we are refering to `http://127.0.0.1:5000` which is 
where the local rest_server has been started.

### Sui Network Endpoints

#### POST /sui/genesis

The `genesis` command creates four authorities and five user accounts
each with five gas objects. These are Sui [objects](objects.md) used
to pay for Sui [transactions](transactions.md#transaction-metadata),
such other object transfers or smart contract (Move) calls.

```shell
curl --location --request POST '{{HOST}}/sui/genesis'
```

#### POST /sui/start

This will start the Sui network with the genesis configuration specified. 

```shell
curl --location --request POST '{{HOST}}/sui/start'
```

#### POST /sui/stop

This will kill the authorities and all of the data stored in the network. Use
this if you want to reset the state of the network without having to kill the 
rest server.

```shell
curl --location --request POST '{{HOST}}/sui/stop'
```

### Sui Endpoints

#### GET /docs

Retrieve OpenAPI documentation.

```shell
curl --location --request GET '{{HOST}}/docs'
```

#### GET /addreses

Retrieve all managed addresses for this client.

```shell
curl --location --request GET '{{HOST}}/addresses'
```

#### GET /objects

Returns the list of objects owned by an address.

```shell
curl --location --request GET '{{HOST}}/objects?address={{address}}'
```

#### GET /object_info

Returns the object information for a specified object.

```shell
curl --location --request GET '{{HOST}}/object_info?objectId={{object_id}}'
```

#### GET /object_schema

Returns the schema for a specified object.

```shell
curl --location --request GET '{{HOST}}/object_schema?objectId={{object_id}}'
```

#### POST /transfer

Transfer object from one address to another. Gas will be paid using the gas
provided in the request. This will be done through a native transfer
transaction that does not require Move VM executions, hence is much cheaper.

```shell
curl --location --request POST '{{HOST}}/transfer' \
--header 'Content-Type: application/json' \
--data-raw '{
    "fromAddress": "{{owner_address}}",
    "objectId": "{{coin}}",
    "toAddress": "{{to_address}}",
    "gasObjectId": "{{gas_object_id}}"
}'
```
Notes:
- Non-coin objects cannot be transferred natively and will require a Move call

#### POST /call

Execute a Move call transaction by calling the specified function in the
module of the given package. Arguments are passed in and type will be
inferred from function signature. Gas usage is capped by the gas_budget.

```shell
curl --location --request POST '{{HOST}}/call' \
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
}'
```
Notes:
- A Publish endpoint is in the works, but for now the only way to add a new module is to have it included as part of genesis. To do this add your Move module to  `sui_programmability/framework/sources` before you hit the genesis endpoint. Once you have done this you will be able to use `"packageObjectId": "0x2"` in the call endpoint to find your Move module.
- To learn more about what `args` are accepted ina Move call, refer to sui-json documentation for further information.

#### POST /sync

Synchronize client state with authorities. This will fetch the latest information
on all objects owned by each address that is managed by this client state.

```shell
curl --location --request POST '{{HOST}}/sync' \
--header 'Content-Type: application/json' \
--data-raw '{
    "address": "{{address}}"
}'
```