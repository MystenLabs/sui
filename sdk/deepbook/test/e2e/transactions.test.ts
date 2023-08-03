// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeEach } from 'vitest';

import {
	TestToolbox,
	setupSuiClient,
	setupPool,
	setupDeepbookAccount,
	depositAsset,
} from './setup';
import { PoolSummary } from '../../src/types/pool';

const DEPOSIT_AMOUNT = 100;

describe('Interacting with the pool', () => {
	let toolbox: TestToolbox;
	let pool: PoolSummary;
	let accountCapId: string;

	beforeEach(async () => {
		toolbox = await setupSuiClient();
		pool = await setupPool(toolbox);
		accountCapId = await setupDeepbookAccount(toolbox);
		depositAsset(toolbox, pool.poolId, accountCapId, DEPOSIT_AMOUNT);
	});

	it('test creating a pool', async () => {
		expect(pool.poolId).toBeDefined();
	});

	it('test creating a custodian account', async () => {
		expect(accountCapId).toBeDefined();
	});

	it('test deposit base asset', async () => {
		expect(accountCapId).toBeDefined();
	});
});
