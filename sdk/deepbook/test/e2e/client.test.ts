// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, it } from 'vitest';

import { DeepBookClient } from '../../src';
import { Level2BookStatusPoint, PoolSummary } from '../../src/types';
import {
	DEFAULT_LOT_SIZE,
	DEFAULT_TICK_SIZE,
	executeTransaction,
	setupDeepbookAccount,
	setupPool,
	setupSuiClient,
	TestToolbox,
} from './setup';

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

	it('test depositing quote asset with account 1', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const tx = await deepbook.deposit(pool.poolId, undefined, DEPOSIT_AMOUNT);
		await executeTransaction(toolbox, tx);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(BigInt(DEPOSIT_AMOUNT));
	});

	it('test depositing base asset with account 2', async () => {
		const resp = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: pool.baseAsset,
		});
		const baseCoin = resp.data[0].coinObjectId;

		const deepbook = new DeepBookClient(toolbox.client, accountCapId2);
		const tx = await deepbook.deposit(pool.poolId, baseCoin, 5n * DEPOSIT_AMOUNT);
		await executeTransaction(toolbox, tx);
	});

	it('test withdrawing quote asset with account 1', async () => {
		expect(accountCapId).toBeDefined();
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const tx = await deepbook.withdraw(pool.poolId, DEPOSIT_AMOUNT, 'quote');
		await executeTransaction(toolbox, tx);
		const resp = await deepbook.getUserPosition(pool.poolId);
		expect(resp.availableQuoteAmount).toBe(0n);
	});

	it('test placing limit order with account 1', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const depositAmount = DEPOSIT_AMOUNT;
		const deposittx = await deepbook.deposit(pool.poolId, undefined, DEPOSIT_AMOUNT);
		await executeTransaction(toolbox, deposittx);
		const position = await deepbook.getUserPosition(pool.poolId);
		expect(position.availableQuoteAmount).toBe(BigInt(depositAmount));

		const totalLocked = LIMIT_ORDER_PRICE * LIMIT_ORDER_QUANTITY;
		const tx = await deepbook.placeLimitOrder(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'bid',
		);
		await executeTransaction(toolbox, tx);

		const position2 = await deepbook.getUserPosition(pool.poolId);
		expect(position2.availableQuoteAmount).toBe(depositAmount - totalLocked);
		expect(position2.lockedQuoteAmount).toBe(totalLocked);
	});

	it('test listing open orders', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const openOrders = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrders.length).toBe(1);
		const { price: oprice, originalQuantity, orderId } = openOrders[0];
		expect(BigInt(oprice)).toBe(LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
		expect(BigInt(originalQuantity)).toBe(LIMIT_ORDER_QUANTITY);

		const { price: priceFromOrderStatus } = (await deepbook.getOrderStatus(pool.poolId, orderId))!;
		expect(priceFromOrderStatus).toBe(oprice);
	});

	it('test getting market price', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const price = await deepbook.getMarketPrice(pool.poolId);
		expect(price.bestBidPrice).toBe(LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
	});

	it('test getting Level 2 Book status, bid side', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId, toolbox.address());
		const status = (await deepbook.getLevel2BookStatus(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			'bid',
		)) as Level2BookStatusPoint[];
		expect(status.length).toBe(1);
		expect(status[0].price).toBe(LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
		expect(status[0].depth).toBe(LIMIT_ORDER_QUANTITY);
	});

	it('test placing market order with Account 2', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId2, toolbox.address());
		const resp = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: pool.baseAsset,
		});
		const baseCoin = resp.data[0].coinObjectId;

		const balanceBefore = BigInt(
			(
				await toolbox.client.getBalance({
					owner: toolbox.address(),
					coinType: pool.baseAsset,
				})
			).totalBalance,
		);

		const tx = await deepbook.placeMarketOrder(
			accountCapId2,
			pool.poolId,
			LIMIT_ORDER_QUANTITY,
			'ask',
			baseCoin,
		);
		await executeTransaction(toolbox, tx);

		// the limit order should be cleared out after matching with the market order
		const openOrders = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrders.length).toBe(0);

		const balanceAfter = BigInt(
			(
				await toolbox.client.getBalance({
					owner: toolbox.address(),
					coinType: pool.baseAsset,
				})
			).totalBalance,
		);
		expect(balanceBefore).toBe(balanceAfter + LIMIT_ORDER_QUANTITY);
	});

	it('test cancelling limit order with account 1', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const tx = await deepbook.placeLimitOrder(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'bid',
		);
		await executeTransaction(toolbox, tx);

		const openOrdersBefore = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrdersBefore.length).toBe(1);
		const { orderId } = openOrdersBefore[0];

		const txForCancel = await deepbook.cancelOrder(pool.poolId, orderId);
		await executeTransaction(toolbox, txForCancel);

		const openOrdersAfter = await deepbook.listOpenOrders(pool.poolId);
		expect(openOrdersAfter.length).toBe(0);
	});

	it('Test parsing sui coin id', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const resp = await toolbox.client.getCoins({
			owner: toolbox.keypair.getPublicKey().toSuiAddress(),
			coinType: pool.baseAsset,
		});
		const baseCoin = resp.data[0].coinObjectId;
		const type = await deepbook.getCoinType(baseCoin);
		expect(type).toBe(resp.data[0].coinType);
	});

	it('Test parsing complex coin id', async () => {
		const deepbook = new DeepBookClient(toolbox.client, accountCapId);
		const resp = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: pool.baseAsset,
		});
		const baseCoin = resp.data[0].coinObjectId;
		const type = await deepbook.getCoinType(baseCoin);
		expect(type).toBe(resp.data[0].coinType);
	});

	it('Test getting level 2 book status, both sides', async () => {
		const deepbook1 = new DeepBookClient(toolbox.client, accountCapId);
		const deepbook2 = new DeepBookClient(toolbox.client, accountCapId2);
		const tx1 = await deepbook1.placeLimitOrder(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'bid',
		);
		await executeTransaction(toolbox, tx1);
		const tx2 = await deepbook2.placeLimitOrder(
			pool.poolId,
			2n * LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'ask',
		);
		await executeTransaction(toolbox, tx2);
		const tx3 = await deepbook2.placeLimitOrder(
			pool.poolId,
			3n * LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			LIMIT_ORDER_QUANTITY,
			'ask',
		);
		await executeTransaction(toolbox, tx3);
		const status = (await deepbook2.getLevel2BookStatus(
			pool.poolId,
			LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			3n * LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE,
			'both',
		)) as Level2BookStatusPoint[][];
		expect(status.length).toBe(2);
		expect(status[0].length).toBe(1);
		expect(status[1].length).toBe(2);
		expect(status[0][0].price).toBe(LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
		expect(status[1][0].price).toBe(2n * LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
		expect(status[1][1].price).toBe(3n * LIMIT_ORDER_PRICE * DEFAULT_TICK_SIZE);
	});
});
