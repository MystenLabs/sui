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

> All `pnpm` commands are intended to be run in the root of the Sui repo. You can also run them within the `sdk/typescript` directory, and remove change `pnpm sdk` to just `pnpm` when running commands.

## Type Doc

You can view the generated [Type Doc](https://typedoc.org/) for the [current release of the SDK](https://www.npmjs.com/package/@mysten/sui.js) at http://typescript-sdk-docs.s3-website-us-east-1.amazonaws.com/.

For the latest docs for the `main` branch, run `pnpm doc` and open the [doc/index.html](doc/index.html) in your browser.

## Testing

To run unit tests

```
pnpm sdk test:unit
```

To run E2E tests against local network

```
pnpm sdk prepare:e2e

// This will run all e2e tests
pnpm sdk test:e2e

// Alternatively you can choose to run only one test file
npx vitest txn-builder.test.ts
```

Troubleshooting:

If you see errors like `ECONNRESET or "socket hang up"`, run `node -v` to make sure your node version is `v18.x.x`. Refer to this [guide](https://blog.logrocket.com/how-switch-node-js-versions-nvm/) to switch node version.

Some more follow up here is if you used homebrew to install node, there could be multiple paths to node on your machine. https://stackoverflow.com/questions/52676244/node-version-not-updating-after-nvm-use-on-mac

To run E2E tests against DevNet

```
VITE_FAUCET_URL='https://faucet.devnet.sui.io:443/gas' VITE_FULLNODE_URL='https://fullnode.devnet.sui.io' pnpm sdk exec vitest e2e
```

## Connecting to Sui Network

The `JsonRpcProvider` class provides a connection to the JSON-RPC Server and should be used for all read-only operations. The default URLs to connect with the RPC server are:

- local: http://127.0.0.1:9000
- DevNet: https://fullnode.devnet.sui.io

```typescript
import { JsonRpcProvider, devnetConnection } from '@mysten/sui.js';
// connect to Devnet
const provider = new JsonRpcProvider(devnetConnection);
// get tokens from the DevNet faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
);
```

For local development, you can run `cargo run --bin sui-test-validator` to spin up a local network with a local validator, a fullnode, and a faucet server.

```typescript
import { JsonRpcProvider, localnetConnection } from '@mysten/sui.js';
// connect to local RPC server
const provider = new JsonRpcProvider(localnetConnection);
// get tokens from the local faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
);
```

You can also construct your own in custom connections, with your own URLs to your fullnode and faucet server

```typescript
import { JsonRpcProvider, Connection } from '@mysten/sui.js';
// Construct your connection:
const connection = new Connection({
  fullnode: 'https://fullnode.devnet.sui.io',
  faucet: 'https://faucet.devnet.sui.io/gas',
});
// connect to a custom RPC server
const provider = new JsonRpcProvider(connection);
// get tokens from a custom faucet server
await provider.requestSuiFromFaucet(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
);
```

## Examples

Fetch objects owned by the address `0xbff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const objects = await provider.getOwnedObjects({
  owner: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
});
```

Fetch object details for the object with id `0xcff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txn = await provider.getObject({
  id: '0xcff6ccc8707aa517b4f1b95750a2a8c666012df3',
  // fetch the object content field
  options: { showContent: true },
});
// You can also fetch multiple objects in one batch request
const txns = await provider.multiGetObjects({
  ids: [
    '0xcff6ccc8707aa517b4f1b95750a2a8c666012df3',
    '0xdff6ccc8707aa517b4f1b95750a2a8c666012df3',
  ],
  // only fetch the object type
  options: { showType: true },
});
```

Fetch transaction details from transaction digests:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txn = await provider.getTransaction({
  digest: '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
  // only fetch the effects field
  options: { showEffects: true },
});
// You can also fetch multiple transactions in one batch request
const txns = await provider.multiGetTransactions({
  digests: [
    '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
    '7mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
  ],
  // fetch both the input transaction data as well as effects
  options: { showInput: true, showEffects: true },
});
```

Fetch transaction events from a transaction digest:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const txEvents = await provider.getEventsByTransaction(
  '6mn5W1CczLwitHCO9OIUbqirNrQ0cuKdyxaNe16SAME=',
);
```

Fetch events by sender address:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const senderEvents = await provider.getEventsBySender(
  '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
);
```

Fetch coins of type `0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC` owned by an address:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
// If coin type is not specified, it defaults to 0x2::sui::SUI
const coins = await provider.getCoins({
  owner: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
  coinType: '0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC',
});
```

Fetch all coin objects owned by an address:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
const allCoins = await provider.getAllCoins({
  owner: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
});
```

Fetch the total coin balance for one coin type, owned by an address:

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();
// If coin type is not specified, it defaults to 0x2::sui::SUI
const coinBalance = await provider.getBalance({
  owner: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3',
  coinType: '0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC',
});
```

For any operations that involves signing or submitting transactions, you should use the `Signer` API. For example:

To transfer a `0x2::coin::Coin<SUI>`:

```typescript
import {
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  Transaction,
} from '@mysten/sui.js';
// Generate a new Ed25519 Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const tx = new Transaction();
tx.transferObjects(
  [tx.object('0x5015b016ab570df14c87649eda918e09e5cc61e0')],
  tx.pure('0xd84058cb73bdeabe123b56632713dcd65e1a6c92'),
);
const result = await signer.signAndExecuteTransaction({ transaction: tx });
console.log({ result });
```

To split the gas coin into another coin:

```typescript
import {
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  Transaction,
} from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const tx = new Transaction();
const coin = tx.splitCoin(tx.gas, tx.pure(1000));
tx.transferObjects([coin], tx.pure(keypair.getPublicKey().toSuiAddress()));
const result = await signer.signAndExecuteTransaction({ transaction: tx });
console.log({ result });
```

To merge two coins:

```typescript
import {
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  Transaction,
} from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const tx = new Transaction();
tx.mergeCoin(tx.object('0x5015b016ab570df14c87649eda918e09e5cc61e0'), [
  tx.object('0xcc460051569bfb888dedaf5182e76f473ee351af'),
]);
const result = await signer.signAndExecuteTransaction({ transaction: tx });
console.log({ result });
```

To make a move call:

```typescript
import {
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  Transaction,
} from '@mysten/sui.js';
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const packageObjectId = '0x...';
const tx = new Transaction();
tx.moveCall({
  target: `${packageObjectId}::nft::mint`,
  arguments: [tx.pure('Example NFT')],
});
const result = await signer.signAndExecuteTransaction({ transaction: tx });
console.log({ result });
```

Subscribe to all events created by transactions sent by account `0xbff6ccc8707aa517b4f1b95750a2a8c666012df3`

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();

// calls RPC method 'sui_subscribeEvent' with params:
// [ { SenderAddress: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3' } ]
const subscriptionId = await provider.subscribeEvent({
  filter: { SenderAddress: '0xbff6ccc8707aa517b4f1b95750a2a8c666012df3' },
  onMessage(event: SuiEventEnvelope) {
    // handle subscription notification message here. This function is called once per subscription message.
  },
});

// later, to unsubscribe
// calls RPC method 'sui_unsubscribeEvent' with params: [ subscriptionId ]
const subFoundAndRemoved = await provider.unsubscribeEvent({
  id: subscriptionId,
});
```

Subscribe to all events created by a package's `nft` module

```typescript
import { JsonRpcProvider } from '@mysten/sui.js';
const provider = new JsonRpcProvider();

const packageObjectId = '0x...';
const devnetNftFilter = {
  All: [
    { EventType: 'MoveEvent' },
    { Package: packageObjectId },
    { Module: 'nft' },
  ],
};
const devNftSub = await provider.subscribeEvent({
  filter: devnetNftFilter,
  onMessage(event: SuiEventEnvelope) {
    // handle subscription notification message here
  },
});
```

To publish a package:

```typescript
import {
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  Transaction,
  normalizeSuiObjectId,
} from '@mysten/sui.js';
const { execSync } = require('child_process');
// Generate a new Keypair
const keypair = new Ed25519Keypair();
const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
const compiledModulesAndDependencies = JSON.parse(
  execSync(
    `${cliPath} move build --dump-bytecode-as-base64 --path ${packagePath}`,
    { encoding: 'utf-8' },
  ),
);
const tx = new Transaction();
tx.publish(
    compiledModulesAndDeps.modules.map((m: any) => Array.from(fromB64(m))),
    compiledModulesAndDeps.dependencies.map((addr: string) => normalizeSuiObjectId(addr)),
);
const result = await signer.signAndExecuteTransaction({ transaction: tx });
console.log({ result });
```

Alternatively, a Secp256k1 can be initiated:

```typescript
import { Secp256k1Keypair, JsonRpcProvider, RawSigner } from '@mysten/sui.js';
// Generate a new Secp256k1 Keypair
const keypair = new Secp256k1Keypair();

const provider = new JsonRpcProvider();
const signer = new RawSigner(keypair, provider);
```
