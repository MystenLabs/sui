// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { OwnedObjectRef } from '../../src/client';
import { Transaction } from '../../src/transactions';
import { CachingTransactionExecutor } from '../../src/transactions/executor/caching';
import { normalizeSuiAddress } from '../../src/utils';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('CachingTransactionExecutor', async () => {
	let toolbox: TestToolbox;
	let packageId: string;
	let rawPackageId: string;
	let executor: CachingTransactionExecutor;
	let parentObjectId: OwnedObjectRef;
	let receiveObjectId: OwnedObjectRef;

	beforeAll(async () => {
		const packagePath = __dirname + '/./data/tto';
		rawPackageId = (await publishPackage(packagePath)).packageId;
		packageId = normalizeSuiAddress(rawPackageId);
	});

	beforeEach(async () => {
		toolbox = await setup();
		executor = new CachingTransactionExecutor({
			client: toolbox.client,
		});
		const tx = new Transaction();
		vi.spyOn(toolbox.client, 'getNormalizedMoveFunction');
		vi.spyOn(toolbox.client, 'multiGetObjects');
		tx.moveCall({
			target: `${packageId}::tto::start`,
			typeArguments: [],
			arguments: [],
		});
		tx.setSender(toolbox.address());
		const x = await executor.signAndExecuteTransaction({
			transaction: tx,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});

		const y = (x.effects?.created)!.map((o) => getOwnerAddress(o))!;
		receiveObjectId = (x.effects?.created)!.filter(
			(o) => !y.includes(o.reference.objectId) && getOwnerAddress(o) !== undefined,
		)[0];
		parentObjectId = (x.effects?.created)!.filter(
			(o) => y.includes(o.reference.objectId) && getOwnerAddress(o) !== undefined,
		)[0];
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it('caches move function definitions', async () => {
		const tx = new Transaction();

		tx.moveCall({
			target: `${packageId}::tto::receiver`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});

		tx.setSender(toolbox.address());

		const result = await executor.signAndExecuteTransaction({
			transaction: tx,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledOnce();
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledWith({
			package: normalizeSuiAddress(packageId),
			module: 'tto',
			function: 'receiver',
		});

		const receiver = await executor.cache.getMoveFunctionDefinition({
			package: normalizeSuiAddress(packageId),
			module: 'tto',
			function: 'receiver',
		});

		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledOnce();

		expect(receiver).toEqual({
			module: 'tto',
			function: 'receiver',
			package: normalizeSuiAddress(packageId),
			parameters: [
				{
					body: {
						datatype: {
							module: 'tto',
							package: rawPackageId,
							type: 'A',
							typeParameters: [],
						},
					},
					ref: '&mut',
				},
				{
					body: {
						datatype: {
							module: 'transfer',
							package: '0x2',
							type: 'Receiving',
							typeParameters: [
								{
									datatype: {
										module: 'tto',
										package: rawPackageId,
										type: 'B',
										typeParameters: [],
									},
								},
							],
						},
					},
					ref: null,
				},
			],
		});

		await executor.buildTransaction({
			transaction: tx,
		});
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledOnce();
	});

	it('caches objects', async () => {
		const tx = new Transaction();
		const obj = tx.moveCall({
			target: `${packageId}::tto::return_`,
			typeArguments: [],
			arguments: [
				tx.object(parentObjectId.reference.objectId),
				tx.object(receiveObjectId.reference.objectId),
			],
		});
		tx.transferObjects([obj], toolbox.address());
		tx.setSender(toolbox.address());
		const loadedIds: string[] = [];

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		const result = await executor.signAndExecuteTransaction({
			transaction: tx,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		expect(result.effects?.status.status).toBe('success');
		expect(loadedIds).toEqual([]);

		const txb2 = new Transaction();
		txb2.transferObjects([txb2.object(receiveObjectId.reference.objectId)], toolbox.address());
		txb2.setSender(toolbox.address());

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		try {
			const result2 = await executor.signAndExecuteTransaction({
				transaction: txb2,
				signer: toolbox.keypair,
				options: {
					showEffects: true,
				},
			});

			expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);
			expect(result2.effects?.status.status).toBe('success');
		} catch (e) {
			console.log('failed transaction:', await txb2.toJSON());
			throw e;
		}

		await executor.reset();

		const txb3 = new Transaction();
		txb3.transferObjects([txb3.object(receiveObjectId.reference.objectId)], toolbox.address());
		txb3.setSender(toolbox.address());

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		try {
			const result3 = await executor.signAndExecuteTransaction({
				transaction: txb3,
				signer: toolbox.keypair,
				options: {
					showEffects: true,
				},
			});
			expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(1);
			expect(result3.effects?.status.status).toBe('success');
		} catch (e) {
			console.log('failed transaction:', await txb3.toJSON());
			throw e;
		}
	});
});

export function getOwnerAddress(o: OwnedObjectRef): string | undefined {
	if (typeof o.owner == 'object' && 'AddressOwner' in o.owner) {
		return o.owner.AddressOwner;
	} else {
		return undefined;
	}
}
