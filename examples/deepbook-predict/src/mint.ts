// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#mint-binary
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT, type ActiveOracle } from './config.js';

// Deposits DUSDC into the manager and mints one binary "up" position, in a
// single PTB. `dusdcCoinId` is a DUSDC coin object owned by the signer.
export async function mintBinaryUp(params: {
	signer: Ed25519Keypair;
	managerId: string;
	oracle: ActiveOracle;
	dusdcCoinId: string;
	depositAmount: bigint; // DUSDC base units (6 decimals)
	quantity: bigint; // position quantity
}) {
	const { signer, managerId, oracle, dusdcCoinId, depositAmount, quantity } =
		params;
	const tx = new Transaction();

	// 1. Split the deposit amount off a DUSDC coin and deposit it into the manager.
	const [deposit] = tx.splitCoins(tx.object(dusdcCoinId), [depositAmount]);
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

	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});
	if (result.$kind === 'FailedTransaction') {
		throw new Error('mint transaction failed');
	}
	return result.Transaction;
}
// docs::/#mint-binary
