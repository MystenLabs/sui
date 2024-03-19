// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from 'vitest';

import { TransactionBlock } from '../../src/transactions';
import { setup } from './utils/setup';

test('subscribeTransaction', async () => {
	const toolbox = await setup();

	expect(
		// eslint-disable-next-line no-async-promise-executor
		new Promise(async (resolve, reject) => {
			try {
				await toolbox.client.subscribeTransaction({
					filter: { FromAddress: toolbox.address() },
					onMessage() {
						resolve(true);
					},
				});

				const tx = new TransactionBlock();
				const [coin] = tx.splitCoins(tx.gas, [tx.pure(1)]);
				tx.transferObjects([coin], tx.pure(toolbox.address()));
				await toolbox.client.signAndExecuteTransactionBlock({
					signer: toolbox.keypair,
					transactionBlock: tx,
				});
			} catch (e) {
				reject(e);
			}
		}),
	).resolves.toBeTruthy();
});
