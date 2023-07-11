// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from 'vitest';
import { setup } from './utils/setup';
import { TransactionBlock } from '../../src';

test('subscribeTransaction', async () => {
	const toolbox = await setup();

	expect(
		// eslint-disable-next-line no-async-promise-executor
		new Promise(async (resolve) => {
			await toolbox.provider.subscribeTransaction({
				filter: { FromAddress: toolbox.address() },
				onMessage() {
					resolve(true);
				},
			});

			const tx = new TransactionBlock();
			const [coin] = tx.splitCoins(tx.gas, [tx.pure(1)]);
			tx.transferObjects([coin], tx.pure(toolbox.address()));
			await toolbox.signer.signAndExecuteTransactionBlock({
				transactionBlock: tx,
			});
		}),
	).resolves.toBeTruthy();
});
