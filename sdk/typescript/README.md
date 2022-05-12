# Sui TypeScript SDK

This is the Sui TypeScript SDK built on the Sui [JSON RPC API](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md). It provides utility classes and functions for applications to sign transactions and interact with the Sui network.

Note that the SDK is still in development mode and some API functions are subject to change.

## Installation

```bash
$ yarn add @mysten/sui.js
```

## Local Development Environment Setup

Follow the [JSON RPC doc](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md) to start a local network and local RPC server

## Usage

The `JsonRpcProvider` class provides a connection to the JSON-RPC Server and should be used for all read-only operations. For example:

Fetch objects owned by the address `0xbff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider('https://gateway.devnet.sui.io:9000/');
const objects = await provider.getOwnedObjectRefs(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

Fetch transaction details from a transaction digest:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider('https://gateway.devnet.sui.io:9000/');
const txn = await provider.getTransaction(
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME='
);
```

For any operations that involves signing or submitting transactions, you should use the `Signer` API. For example:

To transfer a Coin<SUI>:

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const signer = new RawSigner(
  keypair,
  new JsonRpcProvider('https://gateway.devnet.sui.io:9000/')
);
const txn = await signer.transferCoin({
  signer: keypair.getPublicKey().toSuiAddress(),
  objectId: '0x5015b016ab570df14c87649eda918e09e5cc61e0',
  gasPayment: '0x0a8c2a0fd59bf41678b2e22c3dd2b84425fb3673',
  gasBudget: 10000,
  recipient: '0xBFF6CCC8707AA517B4F1B95750A2A8C666012DF3',
});
```

To sign a raw message:
TODO
