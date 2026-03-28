// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Example: NFT operations with the Sui TypeScript SDK
 *
 * This example demonstrates how to work with NFTs on Sui,
 * including querying, transferring, and managing NFT collections.
 */

import { JsonRpcProvider } from '../src/providers/json-rpc-provider';
import { RawSigner } from '../src/signers/raw-signer';
import { Ed25519Keypair } from '../src/cryptography/ed25519-keypair';

interface NftData {
  id: string;
  name?: string;
  description?: string;
  imageUrl?: string;
  owner: string;
}

async function main() {
  // Set up provider and signer
  const provider = new JsonRpcProvider('https://fullnode.devnet.sui.io:443');
  const keypair = Ed25519Keypair.generate();
  const signer = new RawSigner(keypair, provider);
  const address = keypair.getPublicKey().toSuiAddress();

  console.log('Owner Address:', address);
  console.log('');

  // Example 1: Get all NFTs owned by an address
  console.log('=== Fetching Owned NFTs ===');

  const ownedObjects = await provider.getOwnedObjects({
    owner: address,
  });

  console.log(`Total owned objects: ${ownedObjects.data.length}`);

  // Filter for NFT-like objects
  const nfts: NftData[] = [];

  for (const obj of ownedObjects.data.slice(0, 10)) {
    // Limit to first 10 for demo
    const objectId = obj.data?.objectId;
    if (!objectId) continue;

    try {
      const details = await provider.getObject({
        id: objectId,
        options: {
          showContent: true,
          showType: true,
          showOwner: true,
        },
      });

      // Check if object has NFT-like fields
      const content = details.data?.content;
      if (content && 'fields' in content) {
        const fields = (content as any).fields;

        // Common NFT field patterns
        if (fields.name || fields.url || fields.image_url) {
          nfts.push({
            id: objectId,
            name: fields.name || 'Unnamed NFT',
            description: fields.description || '',
            imageUrl: fields.url || fields.image_url || '',
            owner: address,
          });
        }
      }
    } catch (error) {
      console.error(`Error fetching object ${objectId}:`, error);
    }
  }

  console.log(`Found ${nfts.length} NFT-like objects`);
  console.log('');

  // Example 2: Display NFT details
  console.log('=== NFT Details ===');

  nfts.forEach((nft, index) => {
    console.log(`NFT ${index + 1}:`);
    console.log(`  ID: ${nft.id}`);
    console.log(`  Name: ${nft.name}`);
    console.log(`  Description: ${nft.description || 'N/A'}`);
    console.log(`  Image: ${nft.imageUrl || 'N/A'}`);
    console.log('');
  });

  // Example 3: Group NFTs by collection (simplified)
  console.log('=== NFT Collections ===');

  const collections = new Map<string, NftData[]>();

  nfts.forEach((nft) => {
    // Extract collection name from NFT name (simplified)
    const collectionName = nft.name?.split('#')[0].trim() || 'Unknown Collection';

    if (!collections.has(collectionName)) {
      collections.set(collectionName, []);
    }
    collections.get(collectionName)!.push(nft);
  });

  collections.forEach((items, collectionName) => {
    console.log(`${collectionName}: ${items.length} NFT(s)`);
  });
  console.log('');

  // Example 4: Transfer NFT (commented out - requires actual NFT)
  /*
  console.log('=== Transferring NFT ===');

  if (nfts.length > 0) {
    const nftToTransfer = nfts[0];
    const recipient = '0x1234567890abcdef1234567890abcdef12345678';

    console.log(`Transferring NFT ${nftToTransfer.id} to ${recipient}`);

    const tx = {
      kind: 'transferObject',
      data: {
        objectId: nftToTransfer.id,
        recipient,
        gasBudget: 1000,
      },
    };

    const result = await signer.signAndExecuteTransaction(tx);
    console.log('Transfer successful!');
    console.log('Transaction digest:', result.digest);
  } else {
    console.log('No NFTs available to transfer');
  }
  */

  // Example 5: Query NFT events
  console.log('=== Querying NFT Events ===');

  try {
    const events = await provider.queryEvents({
      query: { MoveModule: { package: '0x2', module: 'transfer' } },
      limit: 5,
    });

    console.log(`Found ${events.data.length} transfer events`);

    events.data.forEach((event, index) => {
      console.log(`Event ${index + 1}:`, event.type);
    });
  } catch (error) {
    console.error('Error querying events:', error);
  }

  console.log('');
  console.log('=== NFT Operations Complete ===');
}

// Run the example
main().catch((error) => {
  console.error('Error:', error);
  process.exit(1);
});
