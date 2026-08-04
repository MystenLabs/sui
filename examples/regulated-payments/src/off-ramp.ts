// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const userKeypair = new Ed25519Keypair();
const providerCollectionAddress = '0xPROVIDER_COLLECTION';
const withdrawalAmount = 1_000_000_000n;

async function notifyProvider(_data: { digest: string; amount: bigint }) {
	// Application-specific provider notification logic.
}

// docs::#off-ramp-send
const tx = new Transaction();

tx.moveCall({
	target: '0x2::balance::send_funds',
	typeArguments: ['0x2::sui::SUI'],
	arguments: [tx.balance({ balance: withdrawalAmount }), tx.pure.address(providerCollectionAddress)],
});

const result = await client.signAndExecuteTransaction({
	signer: userKeypair,
	transaction: tx,
});

// Send the transaction digest to the provider for reconciliation.
const digest = (result.Transaction ?? result.FailedTransaction)!.digest;
await notifyProvider({ digest, amount: withdrawalAmount });
// docs::/#off-ramp-send
