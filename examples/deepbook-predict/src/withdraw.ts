// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#withdraw
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT } from './config.js';

export async function withdrawLiquidity(params: {
	signer: Ed25519Keypair;
	plpCoinId: string;
}) {
	const { signer, plpCoinId } = params;
	const tx = new Transaction();

	const quote = tx.moveCall({
		target: `${PREDICT.packageId}::predict::withdraw`,
		typeArguments: [PREDICT.quoteType],
		arguments: [tx.object(PREDICT.predictObjectId), tx.object(plpCoinId), tx.object.clock()],
	});
	tx.transferObjects([quote], signer.toSuiAddress());

	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});
	if (result.$kind === 'FailedTransaction') throw new Error('withdraw failed');
	return result.Transaction;
}
// docs::/#withdraw
