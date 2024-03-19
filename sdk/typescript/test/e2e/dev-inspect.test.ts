// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, it } from 'vitest';

import { SuiClient } from '../../src/client';
import { Keypair } from '../../src/cryptography';
import { TransactionBlock } from '../../src/transactions';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test dev inspect', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	beforeAll(async () => {
		toolbox = await setup();
		const packagePath = __dirname + '/./data/serializer';
		({ packageId } = await publishPackage(packagePath));
	});

	it('Dev inspect split + transfer', async () => {
		const tx = new TransactionBlock();
		const coin = tx.splitCoins(tx.gas, [tx.pure(10)]);
		tx.transferObjects([coin], tx.pure(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success');
	});

	it('can set gas price as number', async () => {
		const tx = new TransactionBlock();
		const coin = tx.splitCoins(tx.gas, [tx.pure(10)]);
		tx.transferObjects([coin], tx.pure(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success', 2000);
	});

	it('can set gas price as bigint', async () => {
		const tx = new TransactionBlock();
		const coin = tx.splitCoins(tx.gas, [tx.pure(10)]);
		tx.transferObjects([coin], tx.pure(toolbox.address()));
		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success', 2000n);
	});

	it('Move Call that returns struct', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const tx = new TransactionBlock();
		const coin_0 = coins.data[0];
		const obj = tx.moveCall({
			target: `${packageId}::serializer_tests::return_struct`,
			typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>'],
			arguments: [tx.pure(coin_0.coinObjectId)],
		});

		// TODO: Ideally dev inspect transactions wouldn't need this, but they do for now
		tx.transferObjects([obj], tx.pure(toolbox.address()));

		await validateDevInspectTransaction(toolbox.client, toolbox.keypair, tx, 'success');
	});

	it('Move Call that aborts', async () => {
		const tx = new TransactionBlock();
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
	transactionBlock: TransactionBlock,
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
