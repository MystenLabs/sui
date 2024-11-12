// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';

import { DeepBookClient } from '../src/index.js'; // Adjust import source accordingly

/// Example to get [price, quantity] for a balance manager
(async () => {
	const env = 'mainnet';

	const balanceManagers = {
		MANAGER_1: {
			address: '0x344c2734b1d211bd15212bfb7847c66a3b18803f3f5ab00f5ff6f87b6fe6d27d',
			tradeCap: '',
		},
	};

	const dbClient = new DeepBookClient({
		address: '0x0',
		env: env,
		client: new SuiClient({
			url: getFullnodeUrl(env),
		}),
		balanceManagers: balanceManagers,
	});

	const pools = ['SUI_USDC']; //, 'DEEP_SUI', 'DEEP_USDC', 'WUSDT_USDC', 'WUSDC_USDC', 'BETH_USDC'];
	for (const pool of pools) {
		const orders = await dbClient.accountOpenOrders(pool, 'MANAGER_1'); // Update the manager accordingly
		const bidOrdersMap = new Map<number, number>();
		const askOrdersMap = new Map<number, number>();

		for (const orderId of orders) {
			const decoded = decodeOrderId(BigInt(orderId));
			const { isBid, price } = decoded;

			const order = await dbClient.getOrder(pool, orderId);
			let remainingQuantity = 0;
			if (order) {
				remainingQuantity = Number(order.quantity) - Number(order.filled_quantity);
			}

			const orderMap = isBid ? bidOrdersMap : askOrdersMap;
			const existingQuantity = orderMap.get(price) || 0;
			orderMap.set(price, existingQuantity + remainingQuantity);
		}

		console.log(`${pool} bid Orders:`, Array.from(bidOrdersMap.entries()));
		console.log(`${pool} ask Orders:`, Array.from(askOrdersMap.entries()));
	}
})();

function decodeOrderId(encodedOrderId: bigint): { isBid: boolean; price: number; orderId: number } {
	const isBid = encodedOrderId >> 127n === 0n;
	const price = Number((encodedOrderId >> 64n) & ((1n << 63n) - 1n));
	const orderId = Number(encodedOrderId & ((1n << 64n) - 1n));

	return { isBid, price, orderId };
}
