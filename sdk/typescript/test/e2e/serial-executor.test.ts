// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeEach } from 'node:test';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { SerialTransactionBlockExecutor, TransactionBlock } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
beforeAll(async () => {
	toolbox = await setup();

	vi.spyOn(toolbox.client, 'multiGetObjects');
	vi.spyOn(toolbox.client, 'getCoins');
});

afterAll(() => {
	vi.clearAllMocks();
});

describe('Caching executor', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetAllMocks();
	});

	it('Executes multiple transactions using the same objects', async () => {
		const executor = new SerialTransactionBlockExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new TransactionBlock();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());

		const result = await executor.executeTransactionBlock({
			transactionBlock: txb,
			options: { showEffects: true },
		});

		const newCoin = result.effects?.created?.find(
			(ref) => result.effects?.gasObject.reference.objectId !== ref.reference.objectId,
		);

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([newCoin!.reference.objectId], toolbox.address());
		const txb3 = new TransactionBlock();
		txb3.transferObjects([newCoin!.reference.objectId], toolbox.address());
		const txb4 = new TransactionBlock();
		txb4.transferObjects([newCoin!.reference.objectId], toolbox.address());

		const results = await Promise.all([
			executor.executeTransactionBlock({
				transactionBlock: txb2,
				options: { showEffects: true },
			}),
			executor.executeTransactionBlock({
				transactionBlock: txb3,

				options: { showEffects: true },
			}),
			executor.executeTransactionBlock({
				transactionBlock: txb4,

				options: { showEffects: true },
			}),
		]);

		const coinVersions = results.map(
			(result) =>
				result.effects?.mutated?.find(
					(ref) => ref.reference.objectId === newCoin!.reference.objectId,
				)?.reference.version,
		);

		expect(coinVersions).toEqual([5, 6, 7]);
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);
		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);
	});

	it('handles invalid version errors by clearing cache', async () => {
		const executor = new SerialTransactionBlockExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new TransactionBlock();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());

		const result = await executor.executeTransactionBlock({
			transactionBlock: txb,
			options: { showEffects: true },
		});

		const newCoin = result.effects?.created?.find(
			(ref) => result.effects?.gasObject.reference.objectId !== ref.reference.objectId,
		);

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([newCoin!.reference.objectId], toolbox.address());
		const txb3 = new TransactionBlock();
		txb3.transferObjects([newCoin!.reference.objectId], toolbox.address());

		await toolbox.client.signAndExecuteTransactionBlock({
			signer: toolbox.keypair,
			transactionBlock: txb2,
		});

		const result2 = await executor.executeTransactionBlock({
			transactionBlock: txb3,
			options: { showEffects: true },
		});

		console.log(result2);
	});
});
