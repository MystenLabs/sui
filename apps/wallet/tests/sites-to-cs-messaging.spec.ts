// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Page } from '@playwright/test';

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';

function getInAppMessage(page: Page, id: string) {
    return page.evaluate(
        (anId) =>
            Promise.race([
                new Promise((r) => setTimeout(() => r(null), 1000)),
                new Promise((r) => {
                    const callBackFN = (msg: MessageEvent) => {
                        if (
                            msg.data.target === 'sui_in-page' &&
                            msg.data.payload.id === anId
                        ) {
                            window.removeEventListener('message', callBackFN);
                            r(msg.data.payload);
                        }
                    };
                    window.addEventListener('message', callBackFN);
                }),
            ]),
        id
    );
}

const noPermissionError = {
    payload: {
        error: true,
        message:
            "Operation not allowed, dapp doesn't have the required permissions",
    },
};

const noAccountError = {
    payload: {
        error: true,
        message: "Cannot read properties of undefined (reading 'account')",
    },
};

test.describe('site to content script messages', () => {
    test.beforeAll(async ({ page, extensionUrl }) => {
        await createWallet(page, extensionUrl);
        await page.close();
    });

    const allTests = [
        ['get accounts', { type: 'get-account' }, noPermissionError],
        [
            'hasPermissions',
            {
                type: 'has-permissions-request',
            },
            {
                payload: {
                    result: false,
                },
            },
        ],
        [
            'execute transaction no account',
            {
                type: 'execute-transaction-request',
            },
            noAccountError,
        ],
        [
            'execute transaction',
            {
                type: 'execute-transaction-request',
                transaction: { account: '0x100' },
            },
            noPermissionError,
        ],
        [
            'sign transaction no account',
            {
                type: 'sign-transaction-request',
            },
            noAccountError,
        ],
        [
            'sign transaction',
            {
                type: 'sign-transaction-request',
                transaction: { account: '0x100' },
            },
            noPermissionError,
        ],
        [
            'sign message',
            {
                type: 'sign-message-request',
                args: {},
            },
            noPermissionError,
        ],
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
                // no response
                expect(await nextMessage).toBeNull();
            }
        });
    }
});
