// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet } from './utils/auth';

const TEST_TIMEOUT = 45 * 1000;
const STAKE_AMOUNT = 100;

test('staking', async ({ page, extensionUrl }) => {
	test.setTimeout(TEST_TIMEOUT);

	await createWallet(page, extensionUrl);

	await page.getByTestId('faucet-request-button').click();
	await expect(page.getByTestId('coin-balance')).not.toHaveText('0SUI');

	await page.getByTestId('stake-and-earn-button').click();
	await page.getByTestId('validator-list-item').first().click();
	await page.getByTestId('select-validator-cta').click();
	await page.getByTestId('stake-amount-input').fill(STAKE_AMOUNT.toString());
	await page.getByRole('button', { name: 'Stake Now' }).click();
	await expect(page.getByTestId('loading-indicator')).not.toBeVisible({
		timeout: TEST_TIMEOUT,
	});
	await expect(page.getByText(/Stake failed/i)).not.toBeVisible();
	await expect(page.getByTestId('overlay-title')).toHaveText('Transaction');

	await page.getByTestId('close-icon').click();
	await expect(page.getByText(`Currently Staked${STAKE_AMOUNT} SUI`)).toBeVisible();

	await page.getByTestId('stake-and-earn-button').click();
	await expect(page.getByText(/Starts Earning now/)).toBeVisible({
		timeout: TEST_TIMEOUT,
	});

	await page.getByTestId('stake-card').click();
	await page.getByTestId('unstake-button').click();
	await page.getByRole('button', { name: 'Unstake Now' }).click();
	await expect(page.getByTestId('loading-indicator')).not.toBeVisible({
		timeout: TEST_TIMEOUT,
	});
	await expect(page.getByText(/Unstake failed/i)).not.toBeVisible();
	await expect(page.getByTestId('overlay-title')).toHaveText('Transaction');
});
