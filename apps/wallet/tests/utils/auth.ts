// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Page } from '@playwright/test';

export const PASSWORD = 'mystenlabs';

export async function createWallet(page: Page, extensionUrl: string) {
	await page.goto(extensionUrl);
	await page.getByRole('link', { name: /Get Started/ }).click();
	await page.getByRole('link', { name: /Create a New Wallet/ }).click();
	await page.getByLabel('Create Password').fill('mystenlabs');
	await page.getByLabel('Confirm Password').fill('mystenlabs');
	await page.locator('label', { has: page.locator('input[type=checkbox]') }).click();
	await page.getByRole('button', { name: /Create Wallet/ }).click();
	await page.locator('label', { has: page.locator('input[type=checkbox]') }).click();
	await page.getByRole('link', { name: /Open Sui Wallet/ }).click();
	await page.getByTestId('bullshark-dismiss').click();
}

export async function importWallet(page: Page, extensionUrl: string, mnemonic: string[]) {
	await page.goto(extensionUrl);
	await page.getByRole('link', { name: /Get Started/ }).click();
	await page.getByRole('link', { name: /Import an Existing Wallet/ }).click();
	const inputs = await page.locator('input[type=password]');
	const inputsCount = await inputs.count();
	for (let i = 0; i < inputsCount; i++) {
		await inputs.nth(i).fill(mnemonic[i]);
	}
	await page.getByRole('button', { name: /Continue/ }).click();
	await page.getByLabel('Create Password').fill(PASSWORD);
	await page.getByLabel('Confirm Password').fill(PASSWORD);
	await page.getByRole('button', { name: /Import/ }).click();
	await page.getByRole('link', { name: /Open Sui Wallet/ }).click();
	await page.getByTestId('bullshark-dismiss').click();
}
