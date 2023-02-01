// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test, type Page } from '@playwright/test';

import { faucet, mint } from './utils/localnet';

async function search(page: Page, text: string) {
    const searchbar = page.getByTestId('search');
    await searchbar.fill(text);
    await searchbar.press('Enter');
}

test('can search for an address', async ({ page }) => {
    const address = await faucet();
    await page.goto('/');
    await search(page, address);
    await expect(page).toHaveURL(`/address/${address}`);
});

test('can search for objects', async ({ page }) => {
    const address = await faucet();
    const tx = await mint(address);

    const { objectId } = tx.effects.effects.created![0].reference;
    await page.goto('/');
    await search(page, objectId);
    await expect(page).toHaveURL(`/object/${objectId}`);
});

test('can search for transaction', async ({ page }) => {
    const address = await faucet();
    const tx = await mint(address);

    const txid = tx.effects.effects.transactionDigest;
    await page.goto('/');
    await search(page, txid);
    await expect(page).toHaveURL(`/transaction/${txid}`);
});
