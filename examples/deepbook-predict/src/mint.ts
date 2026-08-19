// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#mint-binary
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT, type ActiveOracle } from './config.js';

// Deposits DUSDC into the manager and mints one binary "up" position, in a
// single PTB. The deposit is sourced from the signer's DUSDC wherever it sits,
// whether that is an address balance or several separate coin objects.
export async function mintBinaryUp(params: {
	signer: Ed25519Keypair;
	managerId: string;
	oracle: ActiveOracle;
	depositAmount: bigint; // DUSDC base units (6 decimals)
	quantity: bigint; // position quantity
}) {
	const { signer, managerId, oracle, depositAmount, quantity } = params;
	const tx = new Transaction();

	// 1. Source the deposit amount in DUSDC and deposit it into the manager.
	const deposit = tx.coin({ balance: depositAmount, type: PREDICT.quoteType });
	tx.moveCall({
		target: `${PREDICT.packageId}::predict_manager::deposit`,
		typeArguments: [PREDICT.quoteType],
		arguments: [tx.object(managerId), deposit],
	});

	// 2. Build the MarketKey for an "up" binary position.
	const key = tx.moveCall({
		target: `${PREDICT.packageId}::market_key::up`,
		arguments: [
			tx.pure.id(oracle.oracleId),
			tx.pure.u64(oracle.expiry),
			tx.pure.u64(oracle.strike),
		],
	});

	// 3. Mint the position, paying from the manager's deposited balance.
	tx.moveCall({
		target: `${PREDICT.packageId}::predict::mint`,
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
			`mint aborted: ${status.success ? 'unknown' : JSON.stringify(status.error)}`,
		);
	}
	return result.Transaction;
}
// docs::/#mint-binary
