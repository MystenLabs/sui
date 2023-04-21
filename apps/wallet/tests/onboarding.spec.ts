// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from './fixtures';
import { createWallet } from './utils/auth';
import { generateKeypair } from './utils/localnet';

test('create new wallet', async ({ page, extensionUrl }) => {
    await createWallet(page, extensionUrl);
    await expect(page.getByTestId('coin-page')).toBeVisible();
});

test('import wallet', async ({ page, extensionUrl }) => {
    const { mnemonic, keypair } = await generateKeypair();

    await page.goto(extensionUrl);
    await page.getByRole('link', { name: /Get Started/ }).click();
    await page.getByRole('link', { name: /Import an Existing Wallet/ }).click();
    await page.getByLabel('Enter your 12-word Recovery Phrase').type(mnemonic);
    await page.getByRole('button', { name: /Continue/ }).click();
    await page.getByLabel('Create Password').fill('mystenlabs');
    await page.getByLabel('Confirm Password').fill('mystenlabs');
    await page.getByRole('button', { name: /Import/ }).click();
    await page.getByRole('link', { name: /Open Sui Wallet/ }).click();
    await expect(page.getByTestId('coin-page')).toBeVisible();
    await expect(
        page.getByText(keypair.getPublicKey().toSuiAddress().slice(0, 6))
    ).toBeVisible();
});
