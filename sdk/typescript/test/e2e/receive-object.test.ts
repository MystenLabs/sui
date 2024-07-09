// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { resolve } from 'path';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { OwnedObjectRef, SuiClient } from '../../src/client';
import type { Keypair } from '../../src/cryptography';
import { Transaction } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

function getOwnerAddress(o: OwnedObjectRef): string | undefined {
	// const owner = getObjectOwner(o);
	if (typeof o.owner == 'object' && 'AddressOwner' in o.owner) {
		return o.owner.AddressOwner;
	} else {
		return undefined;
	}
}

describe('Transfer to Object', () => {
	let toolbox: TestToolbox;
	let packageId: string;
	let parentObjectId: OwnedObjectRef;
	let receiveObjectId: OwnedObjectRef;
	let sharedObjectId: string;

	beforeAll(async () => {
		toolbox = await setup();
		packageId = await toolbox.getPackage(resolve(__dirname, './data/tto'));
	});

	beforeEach(async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::start`,
			typeArguments: [],
			arguments: [],
		});
		const x = await validateTransaction(toolbox.client, toolbox.keypair, tx);
		const y = x.effects?.created!.map((o) => getOwnerAddress(o))!;
		receiveObjectId = x.effects?.created!.filter(
			(o) => !y.includes(o.reference.objectId) && getOwnerAddress(o) !== undefined,
		)[0]!;
		parentObjectId = x.effects?.created!.filter(
			(o) => y.includes(o.reference.objectId) && getOwnerAddress(o) !== undefined,
		)[0]!;
		const sharedObject = x.effects?.created!.filter((o) => getOwnerAddress(o) === undefined)[0]!;
		sharedObjectId = sharedObject.reference.objectId;
	});

	it('Basic Receive: receive and then transfer', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::receiver`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});

	it('Basic Receive: receive and then delete', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::deleter`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});

	it('receive + return, then delete', async () => {
		const tx = new Transaction();
		const b = tx.moveCall({
			target: `${packageId}::tto::return_`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		tx.moveCall({
			target: `${packageId}::tto::delete_`,
			typeArguments: [],
			arguments: [b],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});

	it('Basic Receive: &Receiving arg type', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::invalid_call_immut_ref`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});

	it('Basic Receive: &mut Receiving arg type', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::invalid_call_mut_ref`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});

	it.fails('Trying to pass shared object as receiving argument', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::tto::receiver`,
			typeArguments: [],
			arguments: [tx.object(parentObjectId.reference.objectId), tx.object(sharedObjectId)],
		});
		await validateTransaction(toolbox.client, toolbox.keypair, tx);
	});
});

async function validateTransaction(client: SuiClient, signer: Keypair, tx: Transaction) {
	tx.setSenderIfNotSet(signer.getPublicKey().toSuiAddress());
	const localDigest = await tx.getDigest({ client });
	const result = await client.signAndExecuteTransaction({
		signer,
		transaction: tx,
		options: {
			showEffects: true,
		},
	});
	expect(localDigest).toEqual(result.digest);
	expect(result.effects?.status.status).toEqual('success');

	await client.waitForTransaction({ digest: result.digest });
	return result;
}
