// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#supply
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT } from './config.js';

export async function supplyLiquidity(params: {
	signer: Ed25519Keypair;
	amount: bigint;
}) {
	const { signer, amount } = params;
	const tx = new Transaction();

	// Sourced from the signer's DUSDC, whether it sits in an address balance or
	// across several coin objects.
	const supply = tx.coin({ balance: amount, type: PREDICT.quoteType });
	const plp = tx.moveCall({
		target: `${PREDICT.packageId}::predict::supply`,
		typeArguments: [PREDICT.quoteType],
		arguments: [tx.object(PREDICT.predictObjectId), supply, tx.object.clock()],
	});
	tx.transferObjects([plp], signer.toSuiAddress());

	const result = await client.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});
	// Wait for finality before acting on the result, so later reads reflect it.
	await client.waitForTransaction({ result });

	if (result.$kind === 'FailedTransaction') {
		// The transaction is onchain and the sender paid gas. Do not retry it.
		const { status } = result.FailedTransaction;
		throw new Error(
			`supply aborted: ${status.success ? 'unknown' : JSON.stringify(status.error)}`,
		);
	}
	return result.Transaction;
}
// docs::/#supply
