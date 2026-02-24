# Sui TypeScript SDK Examples

This directory contains practical examples demonstrating how to use the Sui TypeScript SDK.

## Examples

### 1. Batch Operations (`batch-operations.ts`)

Demonstrates how to perform multiple operations in parallel for better performance:
- Fetching multiple objects concurrently
- Checking balances for multiple addresses
- Batch transfer preparation
- Transaction monitoring
- Parallel event queries

**Usage:**
```bash
ts-node examples/batch-operations.ts
```

### 2. NFT Operations (`nft-operations.ts`)

Shows how to work with NFTs on Sui:
- Fetching NFTs owned by an address
- NFT metadata extraction and display
- Collection grouping and organization
- NFT transfer operations
- Event querying for NFT transfers

**Usage:**
```bash
ts-node examples/nft-operations.ts
```

## Prerequisites

- Node.js 14 or higher
- TypeScript
- ts-node (optional, for running examples directly)

## Installation

```bash
# Install dependencies
npm install

# Install ts-node globally (optional)
npm install -g ts-node
```

## Running Examples

### Using ts-node:
```bash
ts-node examples/batch-operations.ts
ts-node examples/nft-operations.ts
```

### Using compiled JavaScript:
```bash
# Compile TypeScript
npm run build

# Run compiled example
node dist/examples/batch-operations.js
```

## Configuration

All examples use the Sui devnet by default. To use a different network, modify the provider URL:

```typescript
// Devnet (default)
const provider = new JsonRpcProvider('https://fullnode.devnet.sui.io:443');

// Testnet
const provider = new JsonRpcProvider('https://fullnode.testnet.sui.io:443');

// Local node
const provider = new JsonRpcProvider('http://127.0.0.1:9000');
```

## Common Patterns

### Creating a Signer

```typescript
import { Ed25519Keypair } from '../src/cryptography/ed25519-keypair';
import { RawSigner } from '../src/signers/raw-signer';
import { JsonRpcProvider } from '../src/providers/json-rpc-provider';

const provider = new JsonRpcProvider('https://fullnode.devnet.sui.io:443');
const keypair = Ed25519Keypair.generate();
const signer = new RawSigner(keypair, provider);
```

### Parallel Operations

```typescript
// Instead of sequential:
const obj1 = await provider.getObject(id1);
const obj2 = await provider.getObject(id2);

// Use parallel:
const [obj1, obj2] = await Promise.all([
  provider.getObject(id1),
  provider.getObject(id2),
]);
```

### Error Handling

```typescript
try {
  const result = await provider.getObject(objectId);
  // Handle success
} catch (error) {
  console.error('Error:', error);
  // Handle error
}
```

## Additional Resources

- [Sui Documentation](https://docs.sui.io)
- [TypeScript SDK Reference](https://github.com/MystenLabs/sui/tree/main/sdk/typescript)
- [Sui Explorer](https://explorer.sui.io)

## Contributing

To add a new example:

1. Create a new `.ts` file in this directory
2. Follow the existing example structure
3. Add documentation to this README
4. Include error handling and comments
5. Test your example on devnet

## License

Copyright (c) 2022, Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
