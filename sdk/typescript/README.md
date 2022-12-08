# Sui TypeScript SDK

This is the Sui TypeScript SDK built on the Sui [JSON RPC API](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md). It provides utility classes and functions for applications to sign transactions and interact with the Sui network.

WARNING: Note that we are still iterating on the RPC and SDK API before TestNet, therefore please expect frequent breaking changes in the short-term. We expect the API to stabilize after the upcoming TestNet launch.

## Working with DevNet

The SDK will be published to [npm registry](https://www.npmjs.com/package/@mysten/sui.js) with the same bi-weekly release cycle as the DevNet validators and [RPC Server](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md). To use the SDK in your project, you can do:

```bash
$ npm install @mysten/sui.js
```

You can also use your preferred npm client, such as yarn or pnpm.

## Working with local network

Note that the `latest` tag for the [published SDK](https://www.npmjs.com/package/@mysten/sui.js) might go out of sync with the RPC server on the `main` branch until the next release. If you're developing against a local network, we recommend using the `experimental`-tagged packages, which contain the latest changes from `main`.

```bash
npm install @mysten/sui.js@experimental
```

Refer to the [JSON RPC](https://github.com/MystenLabs/sui/blob/main/doc/src/build/json-rpc.md) topic for instructions about how to start a local network and local RPC server.

## Building Locally

To get started you need to install [pnpm](https://pnpm.io/), then run the following command:

```bash
# Install all dependencies
$ pnpm install
# Run the build for the TypeScript SDK
$ pnpm sdk build
```

## Type Doc

You can view the generated [Type Doc](https://typedoc.org/) for the [current release of the SDK](https://www.npmjs.com/package/@mysten/sui.js) at http://typescript-sdk-docs.s3-website-us-east-1.amazonaws.com/.

For the latest docs for the `main` branch, run `pnpm doc` and open the [doc/index.html](doc/index.html) in your browser.

## Testing

To run unit tests

```
cd sdk/typescript
pnpm run test:unit
```

To run E2E tests against local network

```
cd sdk/typescript
pnpm run prepare:e2e
pnpm run test:e2e
```

To run E2E tests against DevNet

```
cd sdk/typescript
VITE_FAUCET_URL='https://faucet.devnet.sui.io:443/gas' VITE_FULLNODE_URL='https://fullnode.devnet.sui.io' vitest e2e
```

## Connecting to Sui Network

The `JsonRpcProvider` class provides a connection to the JSON-RPC Server and should be used for all read-only operations. The default URLs to connect with the RPC server are:

- local: http://127.0.0.1:9000
- DevNet: https://fullnode.devnet.sui.io

```typescript
import { JsonRpcProvider, Network } from '@mysten/sui.js';
// connect to Devnet
const provider = new JsonRpcProvider(Network.DEVNET);
// get tokens from the DevNet faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

For local development, you can run `cargo run --bin sui-test-validator` to spin up a local network with a local validator, a fullnode, and a faucet server.

```typescript
import { JsonRpcProvider, Network } from '@mysten/sui.js';
// connect to local RPC server
const provider = new JsonRpcProvider(Network.LOCAL);
// get tokens from the local faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

You can also pass in custom URLs to your own fullnode and faucet server

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
// connect to a custom RPC server
const provider = new JsonRpcProvider('https://fullnode.devnet.sui.io', {
  // you can also skip providing this field if you don't plan to interact with the faucet
  faucetURL: 'https://faucet.devnet.sui.io/gas',
});
// get tokens from a custom faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

## Examples

Fetch objects owned by the address `0xbff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const objects = await provider.getOwnedObjectRefs(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

Fetch object details for the object with id `0xcff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txn = await provider.getObject(
  '0xcff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
// You can also fetch multiple objects in one batch request
const txns = await provider.getObjectBatch([
  '0xcff6ccc8707aa517b4f1b95750a2a8c666012df3',
  '0xdff6ccc8707aa517b4f1b95750a2a8c666012df3',
]);
```

Fetch transaction details from transaction digests:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txn = await provider.getTransactionWithEffects(
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME='
);
// You can also fetch multiple transactions in one batch request
const txns = await provider.getTransactionWithEffectsBatch([
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
  '7mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
]);
```

Fetch transaction events from a transaction digest:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txEvents = await provider.getEventsByTransaction(
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME='
);
```

Fetch events by sender address:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const senderEvents = await provider.getEventsBySender(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3'
);
```

For any operations that involves signing or submitting transactions, you should use the `Signer` API. For example:

To transfer a `0x2::coin::Coin<SUI>`:

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Ed25519 Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const transferTxn = await signer.transferObject({
  objectId: '0x5015b016ab570df14c87649eda918e09e5cc61e0',
  gasBudget: 1000,
  recipient: '0xd84058cb73bdeabe123b56632713dcd65e1a6c92',
});
console.log('transferTxn', transferTxn);
```

To split a `0x2::coin::Coin<SUI>` into multiple coins

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const splitTxn = await signer.splitCoin({
  coinObjectId: '0x5015b016ab570df14c87649eda918e09e5cc61e0',
  // Say if the original coin has a balance of 100,
  // This function will create three new coins of amount 10, 20, 30,
  // respectively, the original coin will retain the remaining balance(40).
  splitAmounts: [10, 20, 30],
  gasBudget: 1000,
});
console.log('SplitCoin txn', splitTxn);
```

To merge two coins:

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const mergeTxn = await signer.mergeCoin({
  primaryCoin: '0x5015b016ab570df14c87649eda918e09e5cc61e0',
  coinToMerge: '0xcc460051569bfb888dedaf5182e76f473ee351af',
  gasBudget: 1000,
});
console.log('MergeCoin txn', mergeTxn);
```

To make a move call:

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const moveCallTxn = await signer.executeMoveCall({
  packageObjectId: '0x2',
  module: 'devnet_nft',
  function: 'mint',
  typeArguments: [],
  arguments: [
    'Example NFT',
    'An NFT created by the wallet Command Line Tool',
    'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
  ],
  gasBudget: 10000,
});
console.log('moveCallTxn', moveCallTxn);
```

Subscribe to all events created by transactions sent by account `0xbff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();

// calls RPC method 'sui_subscribeEvent' with params:
// [ { SenderAddress: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3' } ]
const subscriptionId = await provider.subscribeEvent(
  { SenderAddress: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3' },
  (event: SuiEventEnvelope) => {
    // handle subscription notification message here. This function is called once per subscription message.
  }
);

// later, to unsubscribe
// calls RPC method 'sui_unsubscribeEvent' with params: [ subscriptionId ]
const subFoundAndRemoved = await provider.unsubscribeEvent(subscriptionId);
```

Subscribe to all events created by the `devnet_nft` module

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();

const devnetNftFilter = {
  All: [
    { EventType: 'MoveEvent' },
    { Package: '0x2' },
    { Module: 'devnet_nft' },
  ],
};
const devNftSub = await provider.subscribeEvent(
  devnetNftFilter,
  (event: SuiEventEnvelope) => {
    // handle subscription notification message here
  }
);
```

To publish a package:

```typescript
import { Ed25519Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
const { execSync } = require('child_process');
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const compiledModules = JSON.parse(
  execSync(
    `${cliPath} move build --dump-bytecode-as-base64 --path ${packagePath}`,
    { encoding: 'utf-8' }
  )
);
const modulesInBytes = compiledModules.map((m) =>
  Array.from(new Base64DataBuffer(m).getData())
);
const publishTxn = await signer.publish({
  compiledModules: modulesInBytes,
  gasBudget: 10000,
});
console.log('publishTxn', publishTxn);
```

Alternatively, a Secp256k1 can be initiated:

```typescript
import { Secp256k1Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Secp256k1 Keypair
const keypair = new Secp256k1Keypair();

const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
```
