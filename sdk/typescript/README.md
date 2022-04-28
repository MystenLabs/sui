# Sui TypeScript SDK

This is the Sui TypeScript SDK built on the Sui [JSON RPC API](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md). It provides utility classes and functions for applications to sign transactions and interact with the Sui network.

Note that the SDK is still in development mode and some API functions are subject to change.

## Installation

We haven't published the npm package yet, so right now you may use [npm link](https://docs.npmjs.com/cli/v8/commands/npm-link) to install it locally.

```bash
cd sui/sdk/typescript
yarn && yarn build && npm link
```

Then:
```bash
cd your/project
npm link sui.js
```

## Local Development Environment Setup

Follow the [JSON RPC doc](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md) to start a local network and local RPC server

## Usage

The `JsonRpcProvider` class provides a connection to the JSON-RPC Server and should be used for all read-only operations. For example:

Fetch objects owned by the address `C5206DD02C86A510C4848516229B02ADDFACBE55`

```typescript
import { JsonRpcProvider } from 'sui.js';
const provider = new JsonRpcProvider('http://127.0.0.1:5001/');
const objects = await provider.getOwnedObjectRefs(
  'C5206DD02C86A510C4848516229B02ADDFACBE55'
);
```

Fetch transaction details from a transaction digest:

```typescript
import { JsonRpcProvider } from 'sui.js';
const provider = new JsonRpcProvider('http://127.0.0.1:5001/');
const txn = await provider.getTransaction(
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME='
);
```

For any operations that involves signing or submitting transactions, you should use the `Signer` API. For example:

To sign a raw message:
TODO

To transfer a Coin<SUI>:
TODO
