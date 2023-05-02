// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getTransactionDigest } from '@mysten/sui.js';
import { expect, test } from '@playwright/test';

import { faucet, split_coin } from './utils/localnet';

test('displays gas breakdown', async ({ page }) => {
    const address = await faucet();
    const tx = await split_coin(address);
    const txid = getTransactionDigest(tx);
    await page.goto(`/txblock/${txid}`);
    await expect(page.getByTestId('gas-breakdown')).toBeVisible();
});
