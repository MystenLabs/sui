// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from './fixtures';
import { generateKeypair } from './utils/localnet';

test('create new wallet', async ({ page, extensionUrl }) => {
    await page.goto(extensionUrl);
    await page.getByRole('link', { name: /Get Started/ }).click();
    await page.getByRole('link', { name: /Create a New Wallet/ }).click();
    await page.getByLabel('Create Password').fill('mystenlabs');
    await page.getByLabel('Confirm Password').fill('mystenlabs');
    // TODO: Clicking checkbox should be improved:
    await page
        .locator('label', { has: page.locator('input[type=checkbox]') })
        .locator('span')
        .nth(0)
        .click();
    await page.getByRole('button', { name: /Create Wallet/ }).click();
    await page.getByRole('button', { name: /Open Sui Wallet/ }).click();
    await expect(page.getByRole('main')).toBeVisible();
});

test('import wallet', async ({ page, extensionUrl }) => {
    const { mnemonic, keypair } = await generateKeypair();

    await page.goto(extensionUrl);
    await page.getByRole('link', { name: /Get Started/ }).click();
    await page.getByRole('link', { name: /Import an Existing Wallet/ }).click();
    await page.getByLabel('Enter Recovery Phrase').fill(mnemonic);
    await page.getByRole('button', { name: /Continue/ }).click();
    await page.getByLabel('Create Password').fill('mystenlabs');
    await page.getByLabel('Confirm Password').fill('mystenlabs');
    await page.getByRole('button', { name: /Import/ }).click();
    await page.getByRole('button', { name: /Open Sui Wallet/ }).click();
    await expect(page.getByRole('main')).toBeVisible();
    await expect(
        page.getByText(keypair.getPublicKey().toSuiAddress().slice(0, 4))
    ).toBeVisible();
});
