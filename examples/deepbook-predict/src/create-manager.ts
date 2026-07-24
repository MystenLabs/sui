// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#create-manager
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { client } from './client.js';
import { PREDICT } from './config.js';

// Creates and shares a PredictManager, then returns its object ID.
export async function createManager(signer: Ed25519Keypair): Promise<string> {
	const tx = new Transaction();
	tx.moveCall({ target: `${PREDICT.packageId}::predict::create_manager` });

	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true, objectTypes: true },
	});

	if (result.$kind === 'FailedTransaction') {
		throw new Error('create_manager transaction failed');
	}

	const objectTypes = result.Transaction?.objectTypes ?? {};
	const managerId = result.Transaction?.effects?.changedObjects?.find(
		(obj) =>
			obj.idOperation === 'Created' &&
			objectTypes[obj.objectId]?.includes('PredictManager'),
	)?.objectId;

	if (!managerId) {
		throw new Error('Could not find created PredictManager in effects');
	}
	return managerId;
}
// docs::/#create-manager
