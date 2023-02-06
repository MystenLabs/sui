// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from '@playwright/test';

import { getCreatedObjects } from '@mysten/sui.js';
import { faucet, mint } from './utils/localnet';

test('can be reached through URL', async ({ page }) => {
    const address = await faucet();
    const tx = await mint(address);

    const { objectId } = getCreatedObjects(tx)![0].reference;
    await page.goto(`/object/${objectId}`);
    await expect(page.getByRole('heading', { name: objectId })).toBeVisible();
});

test.describe('Owned Objects', () => {
    test('link going from address to object and back', async ({ page }) => {
        const address = await faucet();
        const tx = await mint(address);

        const [nft] = getCreatedObjects(tx)!;
        await page.goto(`/address/0x${address}`);

        // Find a reference to the NFT:
        await page.getByText(nft.reference.objectId.slice(0, 4)).click();
        await expect(page).toHaveURL(`/object/${nft.reference.objectId}`);

        // Find a reference to the owning address:
        await page.getByText(address).click();
        await expect(page).toHaveURL(`/address/0x${address}`);
    });
});
