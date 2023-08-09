// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from '@playwright/test';

import { faucet, split_coin } from './utils/localnet';

test('can be reached through URL', async ({ page }) => {
	const address = await faucet();
	const tx = await split_coin(address);

	const { objectId } = tx.effects!.created![0].reference;
	await page.goto(`/object/${objectId}`);
	await expect(page.getByRole('heading', { name: objectId })).toBeVisible();
});

test.describe('Owned Objects', () => {
	test('link going from address to object and back', async ({ page }) => {
		const address = await faucet();
		const tx = await split_coin(address);

		const [new_coin] = tx.effects!.created!;
		await page.goto(`/address/${address}`);

		// Find a reference to the Coin:
		await page.goto(`/objects/${new_coin.reference.objectId}`);

		// Find a reference to the owning address:
		await page.getByText(address.slice(0, 4)).click();
		await expect(page).toHaveURL(`/address/${address}`);
	});
});
