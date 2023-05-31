// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';
import { generateAddress, generateKeypairFromMnemonic } from './utils/localnet';

const mnemonic = [
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
    'beef',
];

test('send 300 SUI and view transaction activity', async ({
    page,
    extensionUrl,
}) => {
    const keypair = await generateKeypairFromMnemonic(mnemonic.join(' '));
    const address = generateAddress(keypair);

    await createWallet(page, extensionUrl);

    await page.getByTestId('faucet-request-button').click();
    await expect(page.getByTestId('coin-balance')).toHaveText('1,000SUI');

    await page.getByTestId('send-coin-button').click();
    await page.getByTestId('coin-amount-input').fill('300');
    await page.getByTestId('address-input').fill(address);
    await page.getByRole('button', { name: 'Review' }).click();
    await page.getByRole('button', { name: 'Send Now' }).click();
    await expect(page.getByTestId('overlay-title')).toHaveText('Transaction');

    await page.getByTestId('close-icon').click();
    await page.getByTestId('nav-tokens').click();
    await expect(page.getByTestId('coin-balance')).toHaveText('700SUI');

    await page.getByTestId('nav-activity').click();
    await page.getByTestId('link-to-txn').first().click();
    await expect(page.getByText('Amount+300 SUI')).toBeVisible();
});
