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
} from './setup';
import { PoolSummary } from '../../src/types';
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

	it('test deposit quote asset', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(BigInt(DEPOSIT_AMOUNT));
	});

	it('test withdraw quote asset', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const txb = await deepbook.withdraw(pool.poolId, DEPOSIT_AMOUNT, 'Quote');
		await executeTransactionBlock(toolbox, txb);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(0n);
	});

	it('test place limit order', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const depositAmount = DEPOSIT_AMOUNT;
		await depositAsset(toolbox, pool.poolId, accountCapId, depositAmount);
		const position = await deepbook.getUserPosition(pool.poolId);
		expect(position.availableQuoteAmount).toBe(BigInt(depositAmount));

		const price = 1n;
		const amount = 1n * DEFAULT_LOT_SIZE;
		const totalLocked = price * amount;
		const txb = await deepbook.placeLimitOrder(
			pool.poolId,
			price * DEFAULT_TICK_SIZE,
			amount,
			true,
		);
		await executeTransactionBlock(toolbox, txb);

		const position2 = await deepbook.getUserPosition(pool.poolId);
		expect(position2.availableQuoteAmount).toBe(depositAmount - totalLocked);
		expect(position2.lockedQuoteAmount).toBe(totalLocked);

		const openOrders = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrders.length).toBe(1);
		const { price: oprice, originalQuantity } = openOrders[0];
		expect(BigInt(oprice)).toBe(price * DEFAULT_TICK_SIZE);
		expect(BigInt(originalQuantity)).toBe(amount);
	});
});
