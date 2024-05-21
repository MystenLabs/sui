// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterAll, beforeAll, beforeEach, describe, expect, it, Mock, vi } from 'vitest';

import { bcs } from '../../src/bcs';
import { SuiClient } from '../../src/client';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { ParallelTransactionExecutor, Transaction } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
beforeAll(async () => {
	toolbox = await setup();

	vi.spyOn(toolbox.client, 'multiGetObjects');
	vi.spyOn(toolbox.client, 'getCoins');
	vi.spyOn(toolbox.client, 'executeTransactionBlock');
});

afterAll(() => {
	vi.restoreAllMocks();
});

describe('ParallelTransactionExecutor', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('Executes multiple transactions in parallel', async () => {
		const executor = new ParallelTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		let concurrentRequests = 0;
		let maxConcurrentRequests = 0;
		let totalTransactions = 0;

		(toolbox.client.executeTransactionBlock as Mock).mockImplementation(async function (
			this: SuiClient,
			input,
		) {
			totalTransactions++;
			concurrentRequests++;
			maxConcurrentRequests = Math.max(maxConcurrentRequests, concurrentRequests);
			const promise = SuiClient.prototype.executeTransactionBlock.call(this, input);

			return promise.finally(() => {
				concurrentRequests--;
			});
		});

		const txbs = Array.from({ length: 10 }, () => {
			const txb = new Transaction();
			txb.transferObjects([txb.splitCoins(txb.gas, [1])[0]], toolbox.address());
			return txb;
		});

		const results = await Promise.all(txbs.map((txb) => executor.executeTransaction(txb)));

		expect(maxConcurrentRequests).toBe(3);
		// 10 + initial coin split + 1 refill to reach concurrency limit
		expect(totalTransactions).toBe(12);

		const digest = new Set(results.map((result) => result.digest));
		expect(digest.size).toBe(results.length);
	});

	it('handles gas coin transfers', async () => {
		const executor = new ParallelTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		const receiver = new Ed25519Keypair();

		const txbs = Array.from({ length: 10 }, () => {
			const txb = new Transaction();
			txb.transferObjects([txb.gas], receiver.toSuiAddress());
			return txb;
		});

		const results = await Promise.all(txbs.map((txb) => executor.executeTransaction(txb)));

		const digest = new Set(results.map((result) => result.digest));
		expect(digest.size).toBe(results.length);

		const returnFunds = new Transaction();
		returnFunds.transferObjects([returnFunds.gas], toolbox.address());

		await toolbox.client.signAndExecuteTransaction({
			transaction: returnFunds,
			signer: receiver,
		});
	});

	it('handles errors', async () => {
		const executor = new ParallelTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		const txbs = Array.from({ length: 10 }, (_, i) => {
			const txb = new Transaction();

			if (i % 2 === 0) {
				txb.transferObjects([txb.splitCoins(txb.gas, [1])[0]], toolbox.address());
			} else {
				txb.moveCall({
					target: '0x123::foo::bar',
					arguments: [],
				});
			}

			return txb;
		});

		const results = await Promise.allSettled(txbs.map((txb) => executor.executeTransaction(txb)));

		const failed = results.filter((result) => result.status === 'rejected');
		const succeeded = new Set(
			results
				.filter((result) => result.status === 'fulfilled')
				.map((r) => (r.status === 'fulfilled' ? r.value.digest : null)),
		);

		expect(failed.length).toBe(5);
		expect(succeeded.size).toBe(5);
	});

	it('handles transactions that use the same objects', async () => {
		const executor = new ParallelTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		const newCoins = await Promise.all(
			new Array(3).fill(null).map(async () => {
				const txb = new Transaction();
				const [coin] = txb.splitCoins(txb.gas, [1]);
				txb.transferObjects([coin], toolbox.address());
				const result = await executor.executeTransaction(txb);

				const effects = bcs.TransactionEffects.fromBase64(result.effects);
				const newCoinId = effects.V2?.changedObjects.find(
					([_id, { outputState }], index) =>
						index !== effects.V2.gasObjectIndex && outputState.ObjectWrite,
				)?.[0]!;

				return newCoinId;
			}),
		);

		const txbs = newCoins.flatMap((newCoinId) => {
			expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);
			const txb2 = new Transaction();
			txb2.transferObjects([newCoinId], toolbox.address());
			const txb3 = new Transaction();
			txb3.transferObjects([newCoinId], toolbox.address());
			const txb4 = new Transaction();
			txb4.transferObjects([newCoinId], toolbox.address());

			return [txb2, txb3, txb4];
		});

		const results = await Promise.all(txbs.map((txb) => executor.executeTransaction(txb)));

		const digests = new Set(results.map((result) => result.digest));

		expect(digests.size).toBe(9);
	});
});
