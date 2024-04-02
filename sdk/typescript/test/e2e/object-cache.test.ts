// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

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
		packageId = normalizeSuiAddress((await publishPackage(packagePath)).packageId);
	});

	beforeEach(async () => {
		toolbox = await setup();
		executor = new CachingTransactionBlockExecutor(toolbox.client);
		const txb = new TransactionBlock();
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

	it('caches move function definitions', async () => {
		const txb = new TransactionBlock();
		const moveFunctionRequests: { package: string; module: string; function: string }[] = [];

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
			dataResolvers: [
				{
					getMoveFunctionDefinition(ref, next) {
						moveFunctionRequests.push(ref);
						return next(ref);
					},
				},
			],
			options: {
				showEffects: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(moveFunctionRequests).toEqual([
			{
				package: packageId,
				module: 'tto',
				function: 'receiver',
			},
		]);

		const receiver = await executor.getMoveFunctionDefinition(
			{ package: packageId, module: 'tto', function: 'receiver' },
			async () => {
				expect.fail('should not be called');
			},
		);

		expect(receiver).toEqual({
			module: 'tto',
			function: 'receiver',
			package: packageId,
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
			dataResolvers: [
				{
					getMoveFunctionDefinition() {
						expect.fail('should not be called');
					},
				},
			],
		});
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

		const result = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
			dataResolvers: [
				{
					getObjects() {
						expect.fail('should not be called');
					},
				},
			],
		});
		expect(result.effects?.status.status).toBe('success');
		expect(loadedIds).toEqual([]);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([txb2.object(receiveObjectId.reference.objectId)], toolbox.address());
		txb2.setSender(toolbox.address());

		const result2 = await executor.signAndExecuteTransactionBlock({
			transactionBlock: txb2,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
			dataResolvers: [
				{
					getObjects() {
						expect.fail('should not be called');
					},
				},
			],
		});
		expect(result2.effects?.status.status).toBe('success');
	});
});

export function getOwnerAddress(o: OwnedObjectRef): string | undefined {
	if (typeof o.owner == 'object' && 'AddressOwner' in o.owner) {
		return o.owner.AddressOwner;
	} else {
		return undefined;
	}
}
