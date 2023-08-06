// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';

import {
	TestToolbox,
	setupSuiClient,
	setupPool,
	setupDeepbookAccount,
	executeTransactionBlock,
	DEFAULT_TICK_SIZE,
	DEFAULT_LOT_SIZE,
} from './setup';
import { PoolSummary } from '../../src/types';
import { DeepBookClient } from '../../src';

const DEPOSIT_AMOUNT = 100n;
const LIMIT_ORDER_PRICE = 1n;
const LIMIT_ORDER_QUANTITY = 1n * DEFAULT_LOT_SIZE;

describe('Interacting with the pool', () => {
	let toolbox: TestToolbox;
	let pool: PoolSummary;
	let accountCapId: string;
	let accountCapId2: string;

	beforeAll(async () => {
		toolbox = await setupSuiClient();
	});

	it('test creating a pool', async () => {
		pool = await setupPool(toolbox);
		expect(pool.poolId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client);
		const pools = await deepbook.getAllPools({});
		expect(pools.data.some((p) => p.poolId === pool.poolId)).toBeTruthy();
	});

	it('test creating a custodian account', async () => {
		accountCapId = await setupDeepbookAccount(toolbox);
		expect(accountCapId).toBeDefined();
		accountCapId2 = await setupDeepbookAccount(toolbox);
		expect(accountCapId2).toBeDefined();
	});

	it('test deposit quote asset with account 1', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const txb = await deepbook.deposit(pool.poolId, undefined, DEPOSIT_AMOUNT);
		await executeTransactionBlock(toolbox, txb);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(BigInt(DEPOSIT_AMOUNT));
	});

	it('test deposit base asset with account 2', async () => {
		const resp = await toolbox.client.getCoins({owner: toolbox.address(), coinType: pool.baseAsset});
		const baseCoin = resp.data[0].coinObjectId;

		const deepbook = new DeepBookClient(toolbox.client, accountCapId2);
		const txb = await deepbook.deposit(pool.poolId, baseCoin, DEPOSIT_AMOUNT);
		await executeTransactionBlock(toolbox, txb);
	});

	it('test withdraw quote asset with account 1', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const txb = await deepbook.withdraw(pool.poolId, DEPOSIT_AMOUNT, 'quote');
		await executeTransactionBlock(toolbox, txb);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(0n);
	});

	it('test place limit order with account 1', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const depositAmount = DEPOSIT_AMOUNT;
		const depositTxb = await deepbook.deposit(pool.poolId, undefined, DEPOSIT_AMOUNT);
		await executeTransactionBlock(toolbox, depositTxb);
		const position = await deepbook.getUserPosition(pool.poolId);
		expect(position.availableQuoteAmount).toBe(BigInt(depositAmount));

	
		const totalLocked = LIMIT_ORDER_PRICE * LIMIT_ORDER_QUANTITY;
		const txb = await deepbook.placeLimitOrder(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'bid',
		);
		await executeTransactionBlock(toolbox, txb);

		const position2 = await deepbook.getUserPosition(pool.poolId);
		expect(position2.availableQuoteAmount).toBe(depositAmount - totalLocked);
		expect(position2.lockedQuoteAmount).toBe(totalLocked);

		const openOrders = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrders.length).toBe(1);
		const { price: oprice, originalQuantity } = openOrders[0];
		expect(BigInt(oprice)).toBe(LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
		expect(BigInt(originalQuantity)).toBe(LIMIT_ORDER_QUANTITY);
	});

	// // expect to run after placing limit order
	it('test place market order with Account 2', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId2, toolbox.address());
		const resp = await toolbox.client.getCoins({owner: toolbox.address(), coinType: pool.baseAsset});
		const baseCoin = resp.data[0].coinObjectId;
		
		const txb = await deepbook.placeMarketOrder(
			pool.poolId,
			LIMIT_ORDER_QUANTITY,
			'ask',
			baseCoin,
		);
		await executeTransactionBlock(toolbox, txb);

		const openOrders = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrders.length).toBe(0);
	});
});
