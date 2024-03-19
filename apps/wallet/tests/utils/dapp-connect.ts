// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BrowserContext, Page } from '@playwright/test';

import { expect } from '../fixtures';

export async function demoDappConnect(page: Page, demoPageUrl: string, context: BrowserContext) {
	await page.goto(demoPageUrl);
	const newWalletPage = context.waitForEvent('page');
	await page.getByRole('button', { name: 'Connect' }).click();
	const walletPage = await newWalletPage;
	await walletPage.waitForLoadState();
	await walletPage.getByRole('button', { name: 'Continue' }).click();
	await walletPage.getByRole('button', { name: 'Connect' }).click();
	const accountsList = page.getByTestId('accounts-list');
	const accountListItems = accountsList.getByRole('listitem');
	await expect(accountListItems).toHaveCount(1);
}
