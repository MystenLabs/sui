// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';

test('wallet unlock', async ({ page, context, extensionUrl }) => {
	await createWallet(page, extensionUrl);
	await page.getByTestId('menu').click();
	await page.getByRole('button', { name: /Lock Wallet/ }).click();
	await page.getByLabel('Enter Password').fill('mystenlabs');
	await page.getByRole('button', { name: /Unlock Wallet/ }).click();
	await expect(page.getByTestId('coin-page')).toBeVisible();
});

test('wallet auto-lock', async ({ page, extensionUrl }) => {
	test.skip(
		process.env.CI !== 'true',
		'Runs only on CI since it takes at least 1 minute to complete',
	);
	test.setTimeout(65 * 1000);
	await createWallet(page, extensionUrl);
	await page.getByTestId('menu').click();
	await page.getByText(/Auto-lock/).click();
	await page.getByPlaceholder(/Auto lock minutes/i).fill('1');
	await page.getByRole('button', { name: /Save/i }).click();
	await page.getByText(/Auto lock updated/i);
	await page.evaluate(() => {
		Object.defineProperty(document, 'visibilityState', {
			value: 'hidden',
		});
		document.dispatchEvent(new Event('visibilitychange'));
	});
	await page.waitForTimeout(60 * 1000);
	await expect(page.getByRole('button', { name: /Unlock Wallet/ })).toBeVisible();
});
