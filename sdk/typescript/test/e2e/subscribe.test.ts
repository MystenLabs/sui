// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from 'vitest';

import { Transaction } from '../../src/transactions';
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

				const tx = new Transaction();
				const [coin] = tx.splitCoins(tx.gas, [1]);
				tx.transferObjects([coin], toolbox.address());
				await toolbox.client.signAndExecuteTransaction({
					signer: toolbox.keypair,
					transaction: tx,
				});
			} catch (e) {
				reject(e);
			}
		}),
	).resolves.toBeTruthy();
});
