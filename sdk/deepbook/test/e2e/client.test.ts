// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';

import {
	TestToolbox,
	setupSuiClient,
	setupPool,
	setupDeepbookAccount,
	depositAsset,
	executeTransactionBlock,
	DEFAULT_TICK_SIZE,
	DEFAULT_LOT_SIZE,
	devInspectTransactionBlock,
} from './setup';
import { PoolSummary } from '../../src/types/pool';
import { DeepBookClient } from '../../src';

const DEPOSIT_AMOUNT = 100n;

describe('Interacting with the pool', () => {
	let toolbox: TestToolbox;
	let pool: PoolSummary;
	let accountCapId: string;

	beforeAll(async () => {
		toolbox = await setupSuiClient();
		pool = await setupPool(toolbox);
		accountCapId = await setupDeepbookAccount(toolbox);
		await depositAsset(toolbox, pool.poolId, accountCapId, DEPOSIT_AMOUNT);
	});

	it('test creating a pool', async () => {
		expect(pool.poolId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client);
		const pools = await deepbook.getAllPools({});
		expect(pools.data.some((p) => p.poolId === pool.poolId)).toBeTruthy();
	});

	it('test creating a custodian account', async () => {
		expect(accountCapId).toBeDefined();
	});

	it('test deposit base asset', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableBaseAmount).toBe(BigInt(DEPOSIT_AMOUNT));
	});

	it('test withdraw base asset', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const txb = await deepbook.withdraw(pool.poolId, DEPOSIT_AMOUNT, 'Base');
		await executeTransactionBlock(toolbox, txb);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableBaseAmount).toBe(0n);
	});

	it('test place limit order', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		await depositAsset(toolbox, pool.poolId, accountCapId, DEPOSIT_AMOUNT);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableBaseAmount).toBe(BigInt(DEPOSIT_AMOUNT));

		const txb = await deepbook.placeLimitOrder(pool.poolId, 1n * DEFAULT_TICK_SIZE, 1n * DEFAULT_LOT_SIZE, true);
		//await executeTransactionBlock(toolbox, txb);

		const resp1 = await devInspectTransactionBlock(toolbox, txb);
		console.log(resp1);
	});
});
