// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { bcs } from '../../src/bcs';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { SerialTransactionExecutor, Transaction } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
beforeAll(async () => {
	toolbox = await setup();

	vi.spyOn(toolbox.client, 'multiGetObjects');
	vi.spyOn(toolbox.client, 'getCoins');
});

afterAll(() => {
	vi.restoreAllMocks();
});

describe('SerialExecutor', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('Executes multiple transactions using the same objects', async () => {
		const executor = new SerialTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new Transaction();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());
		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(0);

		const result = await executor.executeTransaction(txb);

		const effects = bcs.TransactionEffects.fromBase64(result.effects);

		const newCoinId = effects.V2?.changedObjects.find(
			([_id, { outputState }], index) =>
				index !== effects.V2.gasObjectIndex && outputState.ObjectWrite,
		)?.[0]!;

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new Transaction();
		txb2.transferObjects([newCoinId], toolbox.address());
		const txb3 = new Transaction();
		txb3.transferObjects([newCoinId], toolbox.address());
		const txb4 = new Transaction();
		txb4.transferObjects([newCoinId], toolbox.address());

		const results = await Promise.all([
			executor.executeTransaction(txb2),
			executor.executeTransaction(txb3),
			executor.executeTransaction(txb4),
		]);

		expect(results[0].digest).not.toEqual(results[1].digest);
		expect(results[1].digest).not.toEqual(results[2].digest);
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);
		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);
	});

	it('Resets cache on errors', async () => {
		const executor = new SerialTransactionExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new Transaction();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());

		const result = await executor.executeTransaction(txb);
		const effects = bcs.TransactionEffects.fromBase64(result.effects);

		const newCoinId = effects.V2?.changedObjects.find(
			([_id, { outputState }], index) =>
				index !== effects.V2.gasObjectIndex && outputState.ObjectWrite,
		)?.[0]!;

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new Transaction();
		txb2.transferObjects([newCoinId], toolbox.address());
		const txb3 = new Transaction();
		txb3.transferObjects([newCoinId], new Ed25519Keypair().toSuiAddress());

		const { digest } = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: txb2,
		});

		await expect(() => executor.executeTransaction(txb3)).rejects.toThrowError();
		await toolbox.client.waitForTransaction({ digest });

		// // Transaction should succeed after cache reset/error
		const result2 = await executor.executeTransaction(txb3);

		expect(result2.digest).not.toEqual(result.digest);
		expect(bcs.TransactionEffects.fromBase64(result2.effects).V2?.status.Success).toEqual(true);
	});
});
