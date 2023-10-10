// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet, importWallet } from './utils/auth';
import { generateKeypair } from './utils/localnet';

test('create new wallet', async ({ page, extensionUrl }) => {
	await createWallet(page, extensionUrl);
	await page.getByRole('navigation').getByRole('link', { name: 'Coins' }).click();
	await expect(page.getByTestId('coin-page')).toBeVisible();
});

test('import wallet', async ({ page, extensionUrl }) => {
	const { mnemonic, keypair } = await generateKeypair();
	importWallet(page, extensionUrl, mnemonic);
	await page.getByRole('navigation').getByRole('link', { name: 'Coins' }).click();
	await expect(
		page.getByText(keypair.getPublicKey().toSuiAddress().slice(0, 6)).first(),
	).toBeVisible();
});
