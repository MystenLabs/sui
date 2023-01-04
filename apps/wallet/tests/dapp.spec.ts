// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { test, expect } from './fixtures';

test('legacy window adapter', async ({ page }) => {
    await page.goto('https://example.com');

    expect(await page.evaluate(() => typeof (window as any).suiWallet)).toBe(
        'object'
    );

    await Promise.all(
        [
            'hasPermissions',
            'requestPermissions',
            'getAccounts',
            'signAndExecuteTransaction',
            'executeMoveCall',
            'executeSerializedMoveCall',
        ].map(async (method) => {
            expect(
                await page.evaluate(
                    (method) => typeof (window as any).suiWallet[method],
                    method
                )
            ).toBe('function');
        })
    );
});
