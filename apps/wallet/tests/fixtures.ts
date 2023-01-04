// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import 'tsconfig-paths/register';
import { test as base, chromium, type BrowserContext } from '@playwright/test';
import path from 'path';

export const test = base.extend<{
    context: BrowserContext;
    extensionUrl: string;
}>({
    // eslint-disable-next-line no-empty-pattern
    context: async ({}, use) => {
        const pathToExtension = path.join(__dirname, '../dist');
        const context = await chromium.launchPersistentContext('', {
            headless: false,
            args: [
                `--disable-extensions-except=${pathToExtension}`,
                `--load-extension=${pathToExtension}`,
                // Ensure userAgent is correctly set in serviceworker:
                '--user-agent=Playwright',
            ],
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
});
export const expect = test.expect;
