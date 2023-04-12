// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Page } from '@playwright/test';

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';

function getInAppMessage(page: Page, id: string) {
    return page.evaluate(
        (anId) =>
            new Promise((resolve, reject) => {
                const callBackFN = (msg: MessageEvent) => {
                    if (
                        msg.data.target === 'sui_in-page' &&
                        msg.data.payload.id === anId
                    ) {
                        window.removeEventListener('message', callBackFN);
                        if (msg.data.payload.payload.error) {
                            reject(msg.data.payload);
                        } else {
                            resolve(msg.data.payload);
                        }
                    }
                };
                window.addEventListener('message', callBackFN);
            }),
        id
    );
}

test.describe('site to content script messages', () => {
    test.beforeAll(async ({ page, extensionUrl }) => {
        await createWallet(page, extensionUrl);
        await page.close();
    });

    const allTests = [
        ['get accounts', { type: 'get-account' }, false],
        [
            'UI get-features',
            {
                type: 'get-features',
            },
            null,
        ],
        [
            'UI create wallet',
            {
                type: 'keyring',
                method: 'create',
                args: {},
            },
            null,
        ],
    ] as const;
    for (const [aLabel, aPayload, result] of allTests) {
        test(aLabel, async ({ context }) => {
            const page = await context.newPage();
            await page.goto('https://example.com');
            const nextMessage = getInAppMessage(page, aLabel);
            await page.evaluate(
                ({ aPayload: payload, aLabel: label }) => {
                    window.postMessage({
                        target: 'sui_content-script',
                        payload: {
                            id: label,
                            payload,
                        },
                    });
                },
                { aPayload, aLabel }
            );
            if (result) {
                expect(await nextMessage).toMatchObject(result);
            } else {
                await expect(nextMessage).rejects.toThrow();
            }
        });
    }
});
