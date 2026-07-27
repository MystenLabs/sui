// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#redeem
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT, type ActiveOracle } from './config.js';

export async function redeemBinaryUp(params: {
	signer: Ed25519Keypair;
	managerId: string;
	oracle: ActiveOracle;
	quantity: bigint;
	permissionless?: boolean; // for settled positions redeemed by anyone
}) {
	const { signer, managerId, oracle, quantity, permissionless } = params;
	const tx = new Transaction();

	const key = tx.moveCall({
		target: `${PREDICT.packageId}::market_key::up`,
		arguments: [
			tx.pure.id(oracle.oracleId),
			tx.pure.u64(oracle.expiry),
			tx.pure.u64(oracle.strike),
		],
	});

	tx.moveCall({
		target: `${PREDICT.packageId}::predict::${permissionless ? 'redeem_permissionless' : 'redeem'}`,
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
	if (result.$kind === 'FailedTransaction') throw new Error('redeem failed');
	return result.Transaction;
}
// docs::/#redeem
