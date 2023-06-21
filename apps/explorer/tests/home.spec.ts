// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from '@playwright/test';

test('home page', async ({ page }) => {
	await page.goto('/');
	await expect(page).toHaveTitle(/Sui Explorer/);
	await expect(page.getByTestId('home-page')).toBeVisible();
});

test('redirects home when visiting an unknown route', async ({ page }) => {
	await page.goto('/unknown-route');
	await expect(page).toHaveURL('/');
});

test('has a go home button', async ({ page }) => {
	await page.goto('/transactions');
	await expect(page.getByTestId('home-page')).not.toBeVisible();
	await page.getByTestId('nav-logo-button').click();
	await expect(page).toHaveURL('/');
});

test('displays the validator table', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByTestId('validators-table')).toBeVisible();
});

test('displays the node map', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByTestId('node-map')).toBeVisible();
});
