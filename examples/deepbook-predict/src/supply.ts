// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#supply
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT } from './config.js';

export async function supplyLiquidity(params: {
	signer: Ed25519Keypair;
	dusdcCoinId: string;
	amount: bigint;
}) {
	const { signer, dusdcCoinId, amount } = params;
	const tx = new Transaction();

	const [supply] = tx.splitCoins(tx.object(dusdcCoinId), [amount]);
	const plp = tx.moveCall({
		target: `${PREDICT.packageId}::predict::supply`,
		typeArguments: [PREDICT.quoteType],
		arguments: [tx.object(PREDICT.predictObjectId), supply, tx.object.clock()],
	});
	tx.transferObjects([plp], signer.toSuiAddress());

	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});
	if (result.$kind === 'FailedTransaction') throw new Error('supply failed');
	return result.Transaction;
}
// docs::/#supply
