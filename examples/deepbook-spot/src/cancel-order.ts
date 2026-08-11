// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#cancel
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Cancel a resting order by its order ID (from `accountOpenOrders`). Cancelling
// returns the order's locked funds to the manager's settled balances, which you
// then withdraw.
export function cancelOrder(
	client: DeepBookTestnetClient,
	poolKey: string,
	managerKey: string,
	orderId: string,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.deepBook.cancelOrder(poolKey, managerKey, orderId));
	return tx;
}
// docs::/#cancel
