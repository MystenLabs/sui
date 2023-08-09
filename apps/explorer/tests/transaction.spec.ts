// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ProgrammableTransaction } from '@mysten/sui.js/client';
import { expect, test } from '@playwright/test';

import { faucet, split_coin } from './utils/localnet';

test('displays gas breakdown', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = tx.digest;
	await page.goto(`/txblock/${txid}`);
	await expect(page.getByTestId('gas-breakdown')).toBeVisible();
});

test('displays inputs', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = tx.digest;
	await page.goto(`/txblock/${txid}`);

	await expect(page.getByTestId('inputs-card')).toBeVisible();

	const programmableTxn = tx.transaction!.data.transaction as ProgrammableTransaction;
	const actualInputsCount = programmableTxn.inputs.length;
	const inputTextRender = actualInputsCount > 1 ? 'Inputs' : 'Input';

	await expect(page.getByText(`${actualInputsCount} ${inputTextRender}`)).toBeVisible();
});

test('displays transactions card', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = tx.digest;
	await page.goto(`/txblock/${txid}`);

	await expect(page.getByTestId('transactions-card')).toBeVisible();

	const programmableTxn = tx.transaction!.data.transaction as ProgrammableTransaction;
	const actualTransactionsCount = programmableTxn.transactions.length;
	const transactionTextRender = actualTransactionsCount > 1 ? 'Transactions' : 'Transaction';

	await expect(page.getByText(`${actualTransactionsCount} ${transactionTextRender}`)).toBeVisible();
});
