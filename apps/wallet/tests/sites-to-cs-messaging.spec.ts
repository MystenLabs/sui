// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Page } from '@playwright/test';

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';
import { demoDappConnect } from './utils/dapp-connect';

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

test.beforeEach(async ({ page, extensionUrl }) => {
    await createWallet(page, extensionUrl);
    await page.close();
});

test.describe('site to content script messages', () => {
    const allTests = [
        ['get accounts', { type: 'get-account' }, false],
        [
            'execute transaction no account',
            {
                type: 'execute-transaction-request',
            },
            false,
        ],
        [
            'sign transaction no account',
            {
                type: 'sign-transaction-request',
            },
            false,
        ],
        [
            'sign message no account',
            {
                type: 'sign-message-request',
            },
            false,
        ],
        [
            'UI get-features',
            {
                type: 'get-features',
            },
            false,
        ],
        [
            'UI create wallet',
            {
                type: 'keyring',
                method: 'create',
                args: {},
            },
            false,
        ],
    ] as const;
    for (const [aLabel, aPayload, result] of allTests) {
        test(aLabel, async ({ context, demoPageUrl }) => {
            const page = await context.newPage();
            await page.goto(demoPageUrl);
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

test.describe('Wallet interface', () => {
    let demoPage: Page;

    test.beforeEach(async ({ context, demoPageUrl }) => {
        demoPage = await context.newPage();
        await demoPage.goto(demoPageUrl);
    });
    test.describe('when not connected', () => {
        test('no account is connected', async () => {
            expect((await demoPage.locator('.account').all()).length).toBe(0);
            await expect(
                demoPage.getByRole('button', { name: 'Connect' })
            ).toBeVisible();
        });
        test('executing a transaction fails', async () => {
            await demoPage
                .getByRole('button', { name: 'Send transaction' })
                .click();
            await expect(demoPage.getByText('Error')).toBeVisible();
        });
        test('signing a transaction fails', async () => {
            await demoPage
                .getByRole('button', { name: 'Sign transaction' })
                .click();
            await expect(demoPage.getByText('Error')).toBeVisible();
        });
        test('signing a message', async () => {
            await demoPage
                .getByRole('button', { name: 'Sign message' })
                .click();
            await expect(demoPage.getByText('Error')).toBeVisible();
        });
    });
    test.describe('when connected', () => {
        test.beforeEach(async ({ context, demoPageUrl }) => {
            await demoDappConnect(demoPage, demoPageUrl, context);
        });
        test('executing transaction works', async ({ context }) => {
            const newPage = context.waitForEvent('page');
            await demoPage
                .getByRole('button', { name: 'Send transaction' })
                .click();
            const walletPage = await newPage;
            const approve = await walletPage.getByRole('button', {
                name: 'Approve',
            });
            await expect(approve).toBeVisible();
            await expect(approve).toBeEnabled();
        });
        test.describe('and using wrong account', () => {
            test.beforeEach(async () => {
                await demoPage.getByLabel('Use wrong account').check();
            });
            test('executing transaction fails', async () => {
                await demoPage
                    .getByRole('button', { name: 'Send transaction' })
                    .click();
                await expect(demoPage.getByText('Error')).toBeVisible();
            });
            test('signing transaction fails', async () => {
                await demoPage
                    .getByRole('button', { name: 'Sign transaction' })
                    .click();
                await expect(demoPage.getByText('Error')).toBeVisible();
            });
            test('signing message fails', async () => {
                await demoPage
                    .getByRole('button', { name: 'Sign message' })
                    .click();
                await expect(demoPage.getByText('Error')).toBeVisible();
            });
        });
    });
});
