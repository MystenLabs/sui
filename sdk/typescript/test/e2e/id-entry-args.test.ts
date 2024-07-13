// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { resolve } from 'path';
import { beforeAll, describe, expect, it } from 'vitest';

import { Transaction } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

describe('Test ID as args to entry functions', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	beforeAll(async () => {
		toolbox = await setup();
		packageId = await toolbox.getPackage(resolve(__dirname, './data/id_entry_args'));
	});

	it('Test ID as arg to entry functions', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::test::test_id`,
			arguments: [tx.pure.id('0x000000000000000000000000c2b5625c221264078310a084df0a3137956d20ee')],
		});
		const result = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: tx,
			options: {
				showEffects: true,
			},
		});
		expect(result.effects?.status.status).toEqual('success');
	});

	it('Test ID as arg to entry functions', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::test::test_id_non_mut`,
			arguments: [tx.pure.id('0x000000000000000000000000c2b5625c221264078310a084df0a3137956d20ee')],
		});
		const result = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: tx,
			options: {
				showEffects: true,
			},
		});
		expect(result.effects?.status.status).toEqual('success');
	});
});
