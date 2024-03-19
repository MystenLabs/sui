// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet, importWallet } from './utils/auth';
import { generateKeypair } from './utils/localnet';

test('create new wallet', async ({ page, extensionUrl }) => {
	await createWallet(page, extensionUrl);
	await page.getByRole('navigation').getByRole('link', { name: 'Home' }).click();
	await expect(page.getByTestId('coin-page')).toBeVisible();
});

test('import wallet', async ({ page, extensionUrl }) => {
	const { mnemonic, keypair } = await generateKeypair();
	importWallet(page, extensionUrl, mnemonic);
	await page.getByRole('navigation').getByRole('link', { name: 'Home' }).click();
	await expect(
		page.getByText(keypair.getPublicKey().toSuiAddress().slice(0, 6)).first(),
	).toBeVisible();
});

test('zkLogin with google', async ({ page, extensionUrl, context }) => {
	await page.goto(extensionUrl);
	const [bg] = context.serviceWorkers();
	await bg.evaluate(() => {
		(globalThis as any).chrome.identity.launchWebAuthFlow = (
			_: { url: string },
			callback: (a: string) => void,
		) => {
			callback(
				'https://iabmfhchcocfljednlcpgnijajedhecc.chromiumapp.org/#id_token=a.eyJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20iLCJhdWQiOiI5NDY3MzEzNTIyNzYtcGs1Z2xjZzhjcW8zOG5kYjM5aDdqMDkzZnBzcGh1c3UuYXBwcy5nb29nbGV1c2VyY29udGVudC5jb20iLCJzdWIiOiIwMDAwMDAwMDAwMDAwMSIsImVtYWlsIjoidGVzdEBnbWFpbC5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwibmFtZSI6IkZpcnN0IExhc3QifQ.a',
			);
		};
	});
	await context.route('**/get_salt', async (route) => {
		await route.fulfill({ json: { salt: '0' } });
	});
	await page.getByLabel('Sign in with Google').click();
	await page.getByRole('button', { name: 'I understand' }).click();
	await expect(page.getByText('0x00f6…c455')).toBeVisible();
});
