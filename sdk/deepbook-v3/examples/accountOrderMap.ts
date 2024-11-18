// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';

import { DeepBookClient } from '../src/index.js'; // Adjust import source accordingly

/// Example to get [price, quantity] for a balance manager
/// Bids sorted in descending order and asks sorted in ascending order
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

	const pools = ['SUI_USDC', 'DEEP_SUI', 'DEEP_USDC', 'WUSDT_USDC', 'WUSDC_USDC', 'BETH_USDC']; // Update pools as needed
	const manager = 'MANAGER_1'; // Update the manager accordingly
	console.log('Manager:', manager);
	for (const pool of pools) {
		const orders = await dbClient.accountOpenOrders(pool, manager);
		const bidOrdersMap = new Map<number, number>();
		const askOrdersMap = new Map<number, number>();

		for (const orderId of orders) {
			const order = await dbClient.getOrderNormalized(pool, orderId);
			if (!order) {
				continue;
			}
			let remainingQuantity = 0;
			if (order) {
				remainingQuantity = Number(order.quantity) - Number(order.filled_quantity);
			}

			const orderMap = order.isBid ? bidOrdersMap : askOrdersMap;
			const orderPrice = Number(order.normalized_price);
			const existingQuantity = orderMap.get(orderPrice) || 0;
			orderMap.set(orderPrice, existingQuantity + remainingQuantity);
		}

		const sortedBidOrders = Array.from(bidOrdersMap.entries()).sort((a, b) => b[0] - a[0]);
		const sortedAskOrders = Array.from(askOrdersMap.entries()).sort((a, b) => a[0] - b[0]);

		console.log(`${pool} bid orders:`, sortedBidOrders);
		console.log(`${pool} ask orders:`, sortedAskOrders);
	}
})();
