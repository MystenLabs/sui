// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from '@playwright/test';

import { faucet } from './utils/localnet';

test('address page', async ({ page }) => {
    const address = await faucet();
    await page.goto(`/address/${address}`);
    await expect(page.getByRole('heading', { name: address })).toBeVisible();
});
