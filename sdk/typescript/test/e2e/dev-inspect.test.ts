// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { resolve } from 'path';
import { beforeAll, describe, expect, it } from 'vitest';

import { SuiClient } from '../../src/client';
import { Keypair } from '../../src/cryptography';
import { Transaction } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

describe('Test dev inspect', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	beforeAll(async () => {
		toolbox = await setup();
		packageId = await toolbox.getPackage(resolve(__dirname, './data/serializer'));
	});

	it('Dev inspect split + transfer', async () => {
		const tx = new Transaction();
		const coin = tx.splitCoins(tx.gas, [10]);
		tx.transferObjects([coin], tx.pure.address(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success');
	});

	it('can set gas price as number', async () => {
		const tx = new Transaction();
		const coin = tx.splitCoins(tx.gas, [10]);
		tx.transferObjects([coin], tx.pure.address(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success', 2000);
	});

	it('can set gas price as bigint', async () => {
		const tx = new Transaction();
		const coin = tx.splitCoins(tx.gas, [10]);
		tx.transferObjects([coin], tx.pure.address(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success', 2000n);
	});

	it('Move Call that returns struct', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const tx = new Transaction();
		const coin_0 = coins.data[0];
		const obj = tx.moveCall({
			target: `${packageId}::serializer_tests::return_struct`,
			typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>'],
			arguments: [tx.object(coin_0.coinObjectId)],
		});

		// TODO: Ideally dev inspect transactions wouldn't need this, but they do for now
		tx.transferObjects([obj], tx.pure.address(toolbox.address()));

		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success');
	});

	it('Move Call that aborts', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::serializer_tests::test_abort`,
			typeArguments: [],
			arguments: [],
		});

		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'failure');
	});
});

async function validateDevInspectTransaction(
	client: SuiClient,
	signer: Keypair,
	transactionBlock: Transaction,
	status: 'success' | 'failure',
	gasPrice?: number | bigint,
) {
	const result = await client.devInspectTransactionBlock({
		transactionBlock,
		sender: signer.getPublicKey().toSuiAddress(),
		gasPrice,
	});
	expect(result.effects.status.status).toEqual(status);
}
