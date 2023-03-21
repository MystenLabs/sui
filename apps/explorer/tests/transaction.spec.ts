// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { expect, test } from '@playwright/test';

import { faucet, mint } from './utils/localnet';
import { getTransactionDigest } from '@mysten/sui.js';

test('displays the transaction timestamp', async ({ page }) => {
    const address = await faucet();
    const tx = await mint(address);
    const txid = getTransactionDigest(tx);
    await page.goto(`/transaction/${txid}`);
    await expect(
        page.getByTestId('transaction-timestamp').locator('div').nth(1)
    ).not.toBeEmpty();
});

test('displays gas breakdown', async ({ page }) => {
    const address = await faucet();
    const tx = await mint(address);
    const txid = getTransactionDigest(tx);
    await page.goto(`/transaction/${txid}`);
    await expect(page.getByTestId('gas-breakdown')).toBeVisible();
});
