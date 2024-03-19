// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import path from 'path';
import { test as base, chromium, type BrowserContext } from '@playwright/test';

const EXTENSION_PATH = path.join(__dirname, '../dist');
const LAUNCH_ARGS = [
	`--disable-extensions-except=${EXTENSION_PATH}`,
	`--load-extension=${EXTENSION_PATH}`,
	// Ensure userAgent is correctly set in serviceworker:
	'--user-agent=Playwright',
];

export const test = base.extend<{
	context: BrowserContext;
	extensionUrl: string;
	demoPageUrl: string;
}>({
	// eslint-disable-next-line no-empty-pattern
	context: async ({}, use) => {
		const context = await chromium.launchPersistentContext('', {
			headless: false,
			args: LAUNCH_ARGS,
		});
		await use(context);
		await context.close();
	},
	extensionUrl: async ({ context }, use) => {
		let [background] = context.serviceWorkers();
		if (!background) {
			background = await context.waitForEvent('serviceworker');
		}

		const extensionId = background.url().split('/')[2];
		const extensionUrl = `chrome-extension://${extensionId}/ui.html`;
		await use(extensionUrl);
	},
	// eslint-disable-next-line no-empty-pattern
	demoPageUrl: async ({}, use) => {
		await use('http://localhost:5181');
	},
});

export const expect = test.expect;
