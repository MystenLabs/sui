// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#execute
import { Transaction } from '@mysten/sui/transactions';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { DeepBookTestnetClient } from './client.js';

// Sign and execute a transaction, then wait for it to be indexed. Every builder
// on this page returns an unsigned Transaction; this is the step that submits
// it.
//
// `waitForTransaction` is the part people skip. Execution returns as soon as the
// transaction is finalized, but the node you read from can still be serving the
// pre-transaction state. Building the next transaction against that stale state
// fails with a version-unavailable error, so chained steps (create a manager,
// then deposit into it, then order against it) must wait between transactions.
export async function signAndExecute(
	client: DeepBookTestnetClient,
	signer: Ed25519Keypair,
	tx: Transaction,
) {
	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true },
	});

	if (result.$kind === 'FailedTransaction') {
		throw new Error(`Transaction failed: ${JSON.stringify(result.FailedTransaction)}`);
	}

	await client.core.waitForTransaction({ digest: result.Transaction.digest });
	return result.Transaction;
}
// docs::/#execute

// docs::#created-object
// Read a created object's ID out of a transaction's effects, matched by type.
// Creating a BalanceManager or a MarginManager gives you no return value to
// capture, so this is how you recover the ID: ask for `objectTypes` alongside
// `effects`, then find the created object whose type contains the name.
export async function executeAndFindCreated(
	client: DeepBookTestnetClient,
	signer: Ed25519Keypair,
	tx: Transaction,
	typeName: string,
): Promise<string> {
	const result = await client.core.signAndExecuteTransaction({
		transaction: tx,
		signer,
		include: { effects: true, objectTypes: true },
	});

	if (result.$kind === 'FailedTransaction') {
		throw new Error(`Transaction failed: ${JSON.stringify(result.FailedTransaction)}`);
	}

	const objectTypes = result.Transaction.objectTypes ?? {};
	const created = result.Transaction.effects?.changedObjects?.find(
		(obj) => obj.idOperation === 'Created' && objectTypes[obj.objectId]?.includes(typeName),
	)?.objectId;

	if (!created) {
		throw new Error(`No created object of type ${typeName} in effects`);
	}

	await client.core.waitForTransaction({ digest: result.Transaction.digest });
	return created;
}
// docs::/#created-object
