// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#mint-range
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT, type ActiveOracle } from './config.js';

export async function mintRange(params: {
	signer: Ed25519Keypair;
	managerId: string;
	oracle: ActiveOracle;
	lowerStrike: bigint;
	higherStrike: bigint;
	quantity: bigint;
}) {
	const { signer, managerId, oracle, lowerStrike, higherStrike, quantity } = params;
	const tx = new Transaction();

	const key = tx.moveCall({
		target: `${PREDICT.packageId}::range_key::new`,
		arguments: [
			tx.pure.id(oracle.oracleId),
			tx.pure.u64(oracle.expiry),
			tx.pure.u64(lowerStrike),
			tx.pure.u64(higherStrike),
		],
	});

	tx.moveCall({
		target: `${PREDICT.packageId}::predict::mint_range`,
		typeArguments: [PREDICT.quoteType],
		arguments: [
			tx.object(PREDICT.predictObjectId),
			tx.object(managerId),
			tx.object(oracle.oracleId),
			key,
			tx.pure.u64(quantity),
			tx.object.clock(),
		],
	});

	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});
	if (result.$kind === 'FailedTransaction') throw new Error('mint_range failed');
	return result.Transaction;
}
// docs::/#mint-range
