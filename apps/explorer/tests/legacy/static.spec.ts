// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * This is a legacy "end-to-end" test that uses hard-coded mocked API responses
 * and mocked runtime modules. This is gradually being replaced with new true
 * end-to-end tests that leverage a local network instead, and should not be added to.
 */

import { test, expect, type Page } from '@playwright/test';

// NOTE: WE configure this externally as well but this ensures all runners use the correct base.
const BASE_URL = 'http://localhost:8080';

// Standardized CSS Selectors
const nftObject = (num: number) => `div#ownedObjects > div:nth-child(${num}) a`;
const ownerButton = 'td#owner button';

const searchText = async (page: Page, text: string) => {
    await page
        .getByPlaceholder('Search by Addresses / Objects / Transactions')
        .type(text);

    await page.locator('#searchBtn').click();
};

test.describe('Transaction Results', () => {
    const successID = 'vQMG8nrGirX14JLfyzy15DrYD3gwRC1eUmBmBzYUsgh';
    test('can be searched', async ({ page }) => {
        await page.goto(`${BASE_URL}/`);
        await searchText(page, successID);
        await expect(page.getByTestId('pageheader')).toContainText(successID);
    });

    test('can be reached through URL', async ({ page }) => {
        await page.goto(`${BASE_URL}/transaction/${successID}`);
        await expect(page.getByTestId('pageheader')).toContainText(successID);
    });

    test('includes the sender time information', async ({ page }) => {
        await page.goto(`${BASE_URL}/transaction/${successID}`);
        await expect(page.getByTestId('transaction-timestamp')).toContainText(
            new Intl.DateTimeFormat('en-US', {
                month: 'short',
                day: 'numeric',
                year: 'numeric',
                hour: 'numeric',
                minute: 'numeric',
            }).format(new Date('Dec 15, 2024, 00:00:00 UTC'))
        );
    });
});

test.describe('Owned Objects have links that enable', () => {
    test('going from object to child object and back', async ({ page }) => {
        await page.goto(`${BASE_URL}/object/player2`);
        await page.locator(nftObject(1)).click();
        await expect(page.locator('#objectID')).toContainText('Image1');
        await page.locator(ownerButton).click();
        await expect(page.locator('#objectID')).toContainText('player2');
    });

    test('going from parent to broken image object and back', async ({
        page,
    }) => {
        const parentValue = 'ObjectWBrokenChild';
        await page.goto(`${BASE_URL}/object/${parentValue}`);
        await page.locator(nftObject(1)).click();
        await expect(page.locator('#noImage')).toBeVisible();
        await page.locator(ownerButton).click();
        await expect(page.getByTestId('loadedImage')).toHaveCount(3);
    });
});

test.describe('Group View', () => {
    test('evaluates balance', async ({ page }) => {
        const address = 'ownsAllAddress';

        await page.goto(`${BASE_URL}/address/${address}`);

        await expect(page.getByTestId('ownedcoinlabel').nth(0)).toContainText(
            'USD'
        );
        await expect(
            page.getByTestId('ownedcoinobjcount').nth(0)
        ).toContainText('2');
        await expect(page.getByTestId('ownedcoinbalance').nth(0)).toContainText(
            '9,007,199.254'
        );

        await expect(page.getByTestId('ownedcoinlabel').nth(1)).toContainText(
            'SUI'
        );
        await expect(
            page.getByTestId('ownedcoinobjcount').nth(1)
        ).toContainText('2');
        await expect(page.getByTestId('ownedcoinbalance').nth(1)).toContainText(
            '0.0000002'
        );
    });
});

// // TODO: This test isn't great, ideally we'd either do some more manual assertions, validate linking,
// // or use visual regression testing.
test.describe('Transactions for ID', () => {
    test('are displayed from and to address', async ({ page }) => {
        const address = 'ownsAllAddress';
        await page.goto(`${BASE_URL}/address/${address}`);
        await page.getByTestId('tx').locator('td').first().waitFor();
    });

    test('are displayed for input and mutated object', async ({ page }) => {
        const address = 'CollectionObject';
        await page.goto(`${BASE_URL}/address/${address}`);
        await page.getByTestId('tx').locator('td').first().waitFor();
    });
});
