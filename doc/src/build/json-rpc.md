---
title: JSON-RPC API Quick Start
---

Welcome to the guide for making remote procedure calls (RPC) to the Sui network. This document walks you through how to connect and interact with the Sui network using the Sui JSON-RPC API. Use the RPC layer to send your dApp transactions to [Sui validators](../learn/architecture/validators.md) for verification.

This guide is useful for developers interested in Sui network interactions via API and should be used in conjunction with the [SuiJSON format](sui-json.md) for aligning JSON inputs with Move Call arguments.

For a similar guide on Sui network interactions via CLI, refer to the [Sui Client CLI](cli-client.md) documentation.

Follow the instructions to [install Sui binaries](install.md#install-sui-binaries).

## Connect to a Sui network

You can connect to a Sui Full node on a Sui network. Follow the guidance in the [Connect to a Sui Network](../build/connect-sui-network.md) topic to start making RPC calls to the Sui network.

To configure your own Sui Full node, see [Configure a Sui Full node](fullnode.md).

## Sui SDKs

You can sign transactions and interact with the Sui network using any of the following:

- [Sui Rust SDK](rust-sdk.md), a collection of Rust language JSON-RPC wrapper and crypto utilities.
- [Sui TypeScript SDK](https://github.com/MystenLabs/sui/tree/main/sdk/typescript) and [reference files](https://www.npmjs.com/package/@mysten/sui.js).
- [Sui API Reference](https://docs.sui.io/sui-jsonrpc) for all available methods.

## Sui JSON-RPC examples

The following sections demonstrate how to use the Sui JSON-RPC API with cURL commands. See the [Sui API Reference](https://docs.sui.io/sui-jsonrpc) for the latest list of all available methods.

### RPC discover

Sui RPC server supports OpenRPC’s [service discovery method](https://spec.open-rpc.org/#service-discovery-method).
A `rpc.discover` method is added to provide documentation describing our JSON-RPC APIs service.

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"rpc.discover","id":1}'
```

### Transfer object

The examples in this section demonstrate how to create transfer transactions. To use the example commands, replace the values between double brackets ({{ example_ID }} with actual values.

Objects IDs for `{{coin_object_id}}` and `{{gas_object_id}}` must
be owned by the address specified for `{{owner_address}}` for the command to succeed. Use [`sui_getOwnedObjects`](#sui_getOwnedObjects) to return object IDs.

**Important:** As a security best practice, you should serialize data from the JSON-RPC service locally in the same location as the signer. This reduces the risk of trusting data from the service directly.

#### Create an unsigned transaction to transfer a Sui coin from one address to another

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_transferObject",
  "params":[
    "{{owner_address}}",
    "{{object_id}}",
    "{{gas_object_id}}",
    {{gas_budget}},
    "{{to_address}}"],
}' | json_pp
```

A response resembles the following:

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "tx_bytes": "VHJhbnNhY3Rpb25EYXRhOjoAAFHe8jecgzoGWyGlZ1sJ2KBFN8aZF7NIkDsM+3X8mrVCa7adg9HnVqUBAAAAAAAAACDOlrjlT0A18D0DqJLTU28ChUfRFtgHprmuOGCHYdv8YVHe8jecgzoGWyGlZ1sJ2KBFN8aZdZnY6h3kyWFtB38Wyg6zjN7KzAcBAAAAAAAAACDxI+LSHrFUxU0G8bPMXhF+46hpchJ22IHlpPv4FgNvGOgDAAAAAAAA="
  }
}
```

#### Sign a transaction using the Sui keytool

```shell
sui keytool sign --address <owner_address> --data <tx_bytes>
```

The keytool creates a key and then returns the signature and public key information.

#### Execute a transaction with a serialized signature

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_executeTransactionBlockSerializedSig",
  "params": [
    "{{tx_bytes}}",
    "{{signature}}",
    "{{request_type}}"
  ]
}' | json_pp
```

`signature` is a Base64 encoded `flag || signature || pubkey`.

Native transfer by `sui_transferObject` supports any object that allows for public transfers. Some objects cannot be transferred natively and require a [Move call](#sui_movecall). See [Transactions](../learn/transactions.md#native-transaction) for more information about native transfers.

### Invoke Move functions

The example command in this section demonstrate how to call Move functions.

#### Execute a Move call transaction

Execute a Move call transaction by calling the specified function in
the module of a given package (smart contracts in Sui are written in
the [Move](move/index.md) language):

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
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
  "id": 1
}' | json_pp
```

Arguments are passed in, and type is inferred from the function
signature. Gas usage is capped by the `gas_budget`. The `transfer`
function is described in more detail in the [Sui CLI client](cli-client.md#calling-move-code) documentation.

The `transfer` function in the `Coin` module serves the same
purpose as ([`sui_transferObject`](#sui_TransferObject)). It is used for illustration purposes, as a native transfer is more efficient.

To learn more about which `args` a Move call accepts, see [SuiJSON](sui-json.md).

### Publish a Move package

```shell
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc":"2.0",
  "method":"sui_publish",
  "params":[
    "{{owner_address}}",
    ["{{vector_of_compiled_modules}}"],
    ["{{vector_of_dependency_ids}}"],
    "{{gas_object_id}}",
     10000
   ],
  "id":1
}' | json_pp
```

This endpoint performs proper verification and linking to make
sure the package is valid. If some modules have [initializers](move/debug-publish.md#module-initializers), these initializers execute in Move (which means new Move objects can be created in the process of publishing a Move package). Gas budget is required because of the need to execute module initializers.

To publish a Move module, you also need to include `{{vector_of_compiled_modules}}` along with the `{{vector_of_dependency_ids}}`. To generate the values for these fields, use the `sui move` command. The `sui move` command supports printing the bytecode as base64 and dependency object IDs:

```
sui move <move-module-path> build --dump-bytecode-as-base64
```

Assuming that the location of the package's sources is in the `PATH_TO_PACKAGE` environment variable an example command resembles the following:

```
sui move $PATH_TO_PACKAGE/my_move_package build --dump-bytecode-as-base64

{
  "modules": "oRzrCwUAAAAJAQAIAggUAxw3BFMKBV1yB88BdAjDAigK6wIFDPACQgAAAQEBAgEDAAACAAEEDAEAAQEBDAEAAQMDAgAABQABAAAGAgEAAAcDBAAACAUBAAEFBwEBAAEKCQoBAgMLCwwAAgwNAQEIAQcODwEAAQgQAQEABAYFBgcICAYJBgMHCwEBCAALAgEIAAcIAwABBwgDAwcLAQEIAAMHCAMBCwIBCAADCwEBCAAFBwgDAQgAAgsCAQkABwsBAQkAAQsBAQgAAgkABwgDAQsBAQkAAQYIAwEFAgkABQMDBwsBAQkABwgDAQsCAQkAAgsBAQkABQdNQU5BR0VEBENvaW4IVHJhbnNmZXIJVHhDb250ZXh0C1RyZWFzdXJ5Q2FwBGJ1cm4EaW5pdARtaW50DHRyYW5zZmVyX2NhcAtkdW1teV9maWVsZA9jcmVhdGVfY3VycmVuY3kGc2VuZGVyCHRyYW5zZmVyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgACAQkBAAEAAAEECwELADgAAgEAAAAICwkSAAoAOAEMAQsBCwAuEQY4AgICAQAAAQULAQsACwI4AwIDAQAAAQQLAAsBOAQCAA==", "oRzrCwUAAAALAQAOAg4kAzJZBIsBHAWnAasBB9IC6QEIuwQoBuMECgrtBB0MigWzAQ29BgYAAAABAQIBAwEEAQUBBgAAAgAABwgAAgIMAQABBAQCAAEBAgAGBgIAAxAEAAISDAEAAQAIAAEAAAkCAwAACgQFAAALBgcAAAwEBQAADQQFAAIVCgUBAAIICwMBAAIWDQ4BAAIXERIBAgYYAhMAAhkCDgEABRoVAwEIAhsWAwEAAgsXDgEAAg0YBQEABgkHCQgMCA8JCQsMCw8MFAYPBgwNDA0PDgkPCQMHCAELAgEIAAcIBQILAgEIAwsCAQgEAQcIBQABBggBAQMEBwgBCwIBCAMLAgEIBAcIBQELAgEIAAMLAgEIBAMLAgEIAwEIAAEGCwIBCQACCwIBCQAHCwcBCQABCAMDBwsCAQkAAwcIBQELAgEJAAEIBAELBwEIAAIJAAcIBQELBwEJAAEIBgEIAQEJAAIHCwIBCQALAgEJAAMDBwsHAQkABwgFAQYLBwEJAAZCQVNLRVQHTUFOQUdFRARDb2luAklEA1NVSQhUcmFuc2ZlcglUeENvbnRleHQHUmVzZXJ2ZQRidXJuBGluaXQObWFuYWdlZF9zdXBwbHkEbWludApzdWlfc3VwcGx5DHRvdGFsX3N1cHBseQtkdW1teV9maWVsZAJpZAtWZXJzaW9uZWRJRAx0cmVhc3VyeV9jYXALVHJlYXN1cnlDYXADc3VpB21hbmFnZWQFdmFsdWUId2l0aGRyYXcPY3JlYXRlX2N1cnJlbmN5Bm5ld19pZAR6ZXJvDHNoYXJlX29iamVjdARqb2luAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgMIAAAAAAAAAAAAAgEOAQECBA8IBhELBwEIABMLAgEIAxQLAgEIBAABAAAIFg4BOAAMBAsBCgAPADgBCgAPAQoECgI4AgwFCwAPAgsECwI4AwwDCwULAwIBAAAAEA8JEgAKADgEDAEKABEKCwEKADgFCwA4BhIBOAcCAgEAAAMECwAQAjgIAgMBAAAFHA4BOAkMBAoEDgI4CCEDDgsAAQsDAQcAJwoADwELATgKCgAPAgsCOAsLBAsADwALAzgMAgQBAAADBAsAEAE4CQIFAQAAAwQLABAAOA0CAQEBAgEDAA==",
  "dependencies": ["0x0000000000000000000000000000000000000000000000000000000000000001"],
}
Build Successful
```

Copy the output base64 representation of the compiled Move module along with the dependency IDs into the
REST publish endpoint.

The command generates a package object that represents the published Move code. You can use the package ID as an argument for subsequent Move calls to functions defined in this package.

**Note:** If your package has dependencies that are unpublished, include the `--with-unpublished-dependencies` flag to have the modules in those packages added to the bytecode.
