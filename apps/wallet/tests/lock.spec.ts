// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';

test('account lock-unlock', async ({ page, context, extensionUrl }) => {
	await createWallet(page, extensionUrl);
	await page.getByTestId('lock-account-button').click();
	await page.getByTestId('unlock-account-button').click();
	await page.getByPlaceholder('Password').fill('mystenlabs');
	await page.getByRole('button', { name: /Unlock/ }).click();
	await expect(page.getByTestId('coin-balance')).toBeVisible();
});

test('wallet auto-lock', async ({ page, extensionUrl }) => {
	test.skip(
		process.env.CI !== 'true',
		'Runs only on CI since it takes at least 1 minute to complete',
	);
	test.setTimeout(65 * 1000);
	await createWallet(page, extensionUrl);
	await page.getByLabel(/Open settings menu/).click();
	await page.getByText(/Auto-lock/).click();
	await page.getByLabel(/Auto-lock after I am inactive for/i).click();
	await page.getByTestId('auto-lock-timer').fill('1');
	await page.getByRole('combobox').click();
	await page.getByRole('option', { name: /Minute/ }).click();
	await page.getByText('Save').click();
	await page.getByText(/Saved/i);
	await page.getByTestId('close-icon').click();
	await page.getByLabel(/Close settings menu/).click();
	await page.waitForTimeout(60 * 1000);
	await expect(page.getByRole('button', { name: /Unlock/ })).toBeVisible();
});
