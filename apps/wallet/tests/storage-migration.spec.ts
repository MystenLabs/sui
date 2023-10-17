// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';

test('do storage migration', async ({ page, extensionUrl }) => {
	await page.goto(extensionUrl);
	await page.evaluate(async () => {
		(globalThis as any).chrome.storage.local.set({
			accountsPublicInfo: {},
			imported_ledger_accounts: [
				{
					address: '0x58e8d44dbf0b002fc7c045c510e6c75f0d60780d85403bf0865a573301060636',
					derivationPath: "m/44'/784'/0'/0'/0'",
					isSelected: false,
					publicKey: 's8lwY5sPtHKMnL+45E6UvHqUgnGvK4xGlPz5okOEjZ0=',
					type: 'LEDGER',
				},
				{
					address: '0xdaf137852dd2c6b2a8f3fd0364021bbdec0094e815f2dfd64cf0fe01c2b91eb2',
					derivationPath: "m/44'/784'/1'/0'/0'",
					isSelected: false,
					publicKey: 'vw+GsG01+5MWfdxFqeMRc4YAgU6vJlBBs9j/MalnYB8=',
					type: 'LEDGER',
				},
			],
			last_account_index: 2,
			'qredo-connections': [
				{
					accessToken: null,
					accounts: [
						{
							address: '0x3268a405e608624af7c07c34044aee2f8b20da054f4e961192805d93f55f0f30',
							chainID: '',
							labels: [],
							network: 'sui',
							publicKey: '1byqnxoDcH/MVbLH8g9HP+9qsybgmm7uYgSBXBkg6y8=',
							ready: true,
							walletID: 'HSXrpMuDWiHmJdQjHMFcBBYRWi1Pyjkxh5EWsnpMY45d',
						},
					],
					apiUrl: 'https://7ba211-api.qredo.net/connect/sui',
					id: '7ed77b5e-963d-4104-a9d8-11e5133a76a1',
					organization: '2VsyH0ml1s6o7AthXYb80gNMAMr',
					origin: 'https://7ba211-test.qredo.net',
					originFavIcon: 'https://7ba211-test.qredo.net/favicon.ico',
					service: 'qredo-testing',
				},
			],
			sui_Env: null,
			sui_Env_RPC: null,
			v: -1,
			vault: {
				data: '{"data":"/y+fvdpw6ps/lJkczX6B5jTC1/LyOGVFjFO06b1wHL6m1HQMk/EnTiFbwiE1MlaTtxN2NdrWouPfnCaMxfEGmg8TPrgreK5d8929JipdITYRNxlZDY7pW6wRB10LQrRkFawXUhg8vnMYeaL/V6G0NKAdsGyW5MyrzYfdADPzUhF3fLw4Gt90MhIdX/QNLRJJAjENY/RuuQHeCi2hC6qEyFXFFot9aBymDVysln9Ti6pfhctcdjdqbfY/PARLz6Uec6df6u+hehkDoBDwhcTysox4la8/WXzPrqs8rbw2d07g/NRZQFXE6Ancd3pE5Hgh7JrDSplCHaswXe1S/rWwismignzwtpDwzEQZT9VSCvfTka0eA+0vI0/lvsNusscgb81GKWuXV49feSBE6CJ7fvQFAji2DybveFj3udYKy3rC4EpdJFwJU+ze2ZO5hlqXcKToPc/x+z/fPXNSfU9rppw0N6T4Fx++IP6q94tvtAz3ZPZGN+ArgjFFYUQhzyuwCtC6TqJ6XdRq3ZlgYb0UNHviThrmpIBCJH/xPeMdeaYD3XFCHetnwTpfkl+xnv8=","iv":"xe674n8dsybLdtK7121KDw==","salt":"f7v0/BRTLtoI+kS3R832pioFR6qtwB6nZgACkco/H5Q="}',
				v: 2,
			},
		});
	});
	await expect(page.getByText('STORAGE MIGRATION IS REQUIRED')).toBeVisible();
	await page.getByPlaceholder('Password').fill('test-password');
	await page.getByRole('button', { name: 'Continue' }).click();
	await page.getByText('Storage migration done');
	await page.getByRole('link', { name: 'Manage' }).click();
	const allAddresses = [
		'0xaea0…af25',
		'0x2d56…23d1',
		'0x6723…5e4c',
		'0x7a13…82b6',
		'0x58e8…0636',
		'0xdaf1…1eb2',
		'0x3268…0f30',
	];
	for (const anAddress of allAddresses) {
		await expect(page.getByText(anAddress).first()).toBeVisible();
	}
});
