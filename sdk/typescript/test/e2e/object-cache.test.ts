// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { OwnedObjectRef } from '../../src/client';
import { CachingTransactionBlockExecutor, TransactionBlock } from '../../src/transactions';
import { normalizeSuiAddress } from '../../src/utils';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('CachingTransactionBlockExecutor', async () => {
	let toolbox: TestToolbox;
	let packageId: string;
	let executor: CachingTransactionBlockExecutor;
	let parentObjectId: OwnedObjectRef;
	let receiveObjectId: OwnedObjectRef;

	beforeAll(async () => {
		const packagePath = __dirname + '/./data/tto';
		packageId = (await publishPackage(packagePath)).packageId;
	});

	beforeEach(async () => {
		toolbox = await setup();
		executor = new CachingTransactionBlockExecutor({
			client: toolbox.client,
			address: toolbox.address(),
		});
		const txb = new TransactionBlock();
		vi.spyOn(toolbox.client, 'getNormalizedMoveFunction');
		vi.spyOn(toolbox.client, 'multiGetObjects');
		txb.moveCall({
			target: `${packageId}::tto::start`,
			typeArguments: [],
			arguments: [],
		});
		txb.setSender(toolbox.address());
		const x = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb,
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
		const txb = new TransactionBlock();

		txb.moveCall({
			target: `${packageId}::tto::receiver`,
			typeArguments: [],
			arguments: [
				txb.object(parentObjectId.reference.objectId),
				txb.object(receiveObjectId.reference.objectId),
			],
		});

		txb.setSender(toolbox.address());

		const result = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledOnce();
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledWith({
			package: packageId,
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
							package: packageId,
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
										package: packageId,
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

		await executor.buildTransactionBlock({
			transactionBlock: txb,
		});
		expect(toolbox.client.getNormalizedMoveFunction).toHaveBeenCalledOnce();
	});

	it('caches objects', async () => {
		const txb = new TransactionBlock();
		const obj = txb.moveCall({
			target: `${packageId}::tto::return_`,
			typeArguments: [],
			arguments: [
				txb.object(parentObjectId.reference.objectId),
				txb.object(receiveObjectId.reference.objectId),
			],
		});
		txb.transferObjects([obj], toolbox.address());
		txb.setSender(toolbox.address());
		const loadedIds: string[] = [];

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		const result = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		expect(result.effects?.status.status).toBe('success');
		expect(loadedIds).toEqual([]);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([txb2.object(receiveObjectId.reference.objectId)], toolbox.address());
		txb2.setSender(toolbox.address());

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		const result2 = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb2,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);
		expect(result2.effects?.status.status).toBe('success');

		await executor.reset();

		const txb3 = new TransactionBlock();
		txb3.transferObjects([txb3.object(receiveObjectId.reference.objectId)], toolbox.address());
		txb3.setSender(toolbox.address());

		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);

		const result3 = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb3,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(1);
		expect(result3.effects?.status.status).toBe('success');
	});
});

export function getOwnerAddress(o: OwnedObjectRef): string | undefined {
	if (typeof o.owner == 'object' && 'AddressOwner' in o.owner) {
		return o.owner.AddressOwner;
	} else {
		return undefined;
	}
}
