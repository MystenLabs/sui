// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#read-orders
import type { DeepBookTestnetClient } from './client.js';

// Read a manager's open orders and account state on a pool. `accountOpenOrders`
// lists resting order IDs. `getOrder` returns per-order fields including
// `quantity` and `filled_quantity`. `account` returns settled and locked
// balances: a fill moves funds into settled balances until you withdraw them.
export async function readOrders(
	client: DeepBookTestnetClient,
	poolKey: string,
	managerKey: string,
) {
	const openOrderIds = await client.deepbook.accountOpenOrders(poolKey, managerKey);
	const orders = await Promise.all(openOrderIds.map((id) => client.deepbook.getOrder(poolKey, id)));
	const account = await client.deepbook.account(poolKey, managerKey);
	return { openOrderIds, orders, account };
}
// docs::/#read-orders
