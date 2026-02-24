// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Example: Batch operations with the Sui TypeScript SDK
 *
 * This example demonstrates how to perform multiple operations
 * in parallel for better performance.
 */

import { JsonRpcProvider } from '../src/providers/json-rpc-provider';
import { RawSigner } from '../src/signers/raw-signer';
import { Ed25519Keypair } from '../src/cryptography/ed25519-keypair';

async function main() {
  // Set up provider and signer
  const provider = new JsonRpcProvider('https://fullnode.devnet.sui.io:443');
  const keypair = Ed25519Keypair.generate();
  const signer = new RawSigner(keypair, provider);

  console.log('Address:', keypair.getPublicKey().toSuiAddress());

  // Example 1: Fetch multiple objects in parallel
  console.log('\n=== Fetching Multiple Objects ===');

  const objectIds = [
    '0x0000000000000000000000000000000000000002',
    '0x0000000000000000000000000000000000000003',
  ];

  const objects = await Promise.all(
    objectIds.map((id) => provider.getObject(id))
  );

  objects.forEach((obj, index) => {
    console.log(`Object ${index + 1}:`, obj.status);
  });

  // Example 2: Check balances for multiple addresses
  console.log('\n=== Checking Multiple Balances ===');

  const addresses = [
    keypair.getPublicKey().toSuiAddress(),
    // Add more addresses here
  ];

  const balances = await Promise.all(
    addresses.map(async (address) => {
      const coins = await provider.getCoins(address);
      return {
        address,
        totalCoins: coins.data.length,
      };
    })
  );

  balances.forEach((balance) => {
    console.log(`${balance.address}: ${balance.totalCoins} coins`);
  });

  // Example 3: Batch transfer preparation
  console.log('\n=== Preparing Batch Transfers ===');

  const recipients = [
    '0x1234567890abcdef1234567890abcdef12345678',
    '0xabcdef1234567890abcdef1234567890abcdef12',
  ];

  const transferAmounts = [1000, 2000];

  console.log('Batch transfer plan:');
  recipients.forEach((recipient, index) => {
    console.log(`  - Send ${transferAmounts[index]} to ${recipient.slice(0, 10)}...`);
  });

  // Example 4: Monitor multiple transactions
  console.log('\n=== Transaction Monitoring ===');

  const digests = [
    // Add transaction digests here
  ];

  if (digests.length > 0) {
    const results = await Promise.all(
      digests.map((digest) => provider.getTransactionBlock({ digest }))
    );

    results.forEach((result, index) => {
      console.log(`Transaction ${index + 1}:`, result.digest);
    });
  } else {
    console.log('No transactions to monitor');
  }

  // Example 5: Parallel event queries
  console.log('\n=== Querying Events ===');

  const eventQueries = [
    { MoveModule: { package: '0x2', module: 'coin' } },
    { MoveModule: { package: '0x2', module: 'sui' } },
  ];

  const eventResults = await Promise.all(
    eventQueries.map((query) =>
      provider.queryEvents({
        query,
        limit: 5,
      })
    )
  );

  eventResults.forEach((events, index) => {
    console.log(`Query ${index + 1}: ${events.data.length} events found`);
  });

  console.log('\n=== Batch Operations Complete ===');
}

// Run the example
main().catch((error) => {
  console.error('Error:', error);
  process.exit(1);
});
