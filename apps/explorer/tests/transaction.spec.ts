// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getTransactionDigest, type ProgrammableTransaction } from '@mysten/sui.js';
import { expect, test } from '@playwright/test';

import { faucet, split_coin } from './utils/localnet';

test('displays gas breakdown', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = getTransactionDigest(tx);
	await page.goto(`/txblock/${txid}`);
	await expect(page.getByTestId('gas-breakdown')).toBeVisible();
});

test('displays inputs', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = getTransactionDigest(tx);
	await page.goto(`/txblock/${txid}`);

	await expect(page.getByTestId('inputs-card')).toBeVisible();

	const programmableTxn = tx.transaction!.data.transaction as ProgrammableTransaction;
	const actualInputsCount = programmableTxn.inputs.length;

	const inputsCardContentsCount = await page.getByTestId(`inputs-card-content`).count();
	await expect(inputsCardContentsCount).toBe(actualInputsCount);
});

test('displays transactions card', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);
	const txid = getTransactionDigest(tx);
	await page.goto(`/txblock/${txid}`);

	await expect(page.getByTestId('transactions-card')).toBeVisible();

	const programmableTxn = tx.transaction!.data.transaction as ProgrammableTransaction;
	const actualTransactionsCount = programmableTxn.transactions.length;

	const transactionsContentCount = await page.getByTestId(`transactions-card-content`).count();
	await expect(transactionsContentCount).toBe(actualTransactionsCount);
});
