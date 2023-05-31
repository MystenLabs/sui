// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Page } from '@playwright/test';

import { expect } from '../fixtures';

export async function getLocalTokens(page: Page) {
    /**
     * Runs this only in local environment for debugging purposes.
     */
    if (process.env.CI !== 'true') {
        await page.getByTestId('menu').click();
        await page.getByTestId('Network').click();
        await page.getByTestId('local').click();
        await page.getByTestId('menu').click();
    }

    await page.getByTestId('faucet-request-button').click();
    await expect(page.getByTestId('coin-balance')).toHaveText('1,000SUI');
}
