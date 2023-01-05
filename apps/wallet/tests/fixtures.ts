// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test as base, chromium, type BrowserContext } from '@playwright/test';
import fs from 'fs';
import os from 'os';
import path from 'path';

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
}>({
    // eslint-disable-next-line no-empty-pattern
    context: async ({}, use) => {
        console.log('making temp dir');
        const tmpUserDataDir = await fs.promises.mkdtemp(
            path.join(os.tmpdir(), 'playwright-user-data-dir-')
        );
        console.log('making persistent context');
        const context = await chromium.launchPersistentContext(tmpUserDataDir, {
            headless: false,
            args: LAUNCH_ARGS,
        });
        console.log('using');
        await use(context);
        console.log('closing');
        await context.close();
        await fs.promises.rm(tmpUserDataDir, { recursive: true, force: true });
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
