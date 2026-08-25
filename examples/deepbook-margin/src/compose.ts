// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { OrderType } from '@mysten/deepbook-v3';
import type { DeepBookMarginClient } from './client.js';

// docs::#lend-trade-stake
// Deploy capital three ways in one transaction: lend to a margin pool, place a
// maker order on the book, and stake DEEP for governance rebates. The three
// legs are independent, but composing them in one programmable transaction block
// makes them atomic: if any leg aborts (for example the order is off-tick or the
// stake amount is zero), none of them apply, so you never lend without also
// getting the order and stake you intended.
//
// The lend leg uses a reusable SupplierCap and pulls its coin from the wallet.
// The trade and stake legs both act on a spot BalanceManager, so keep enough
// DEEP in the manager to cover the order and the stake together.
export interface LendTradeStakeParams {
	balanceManagerKey: string;
	supplierCapId: string;
	lendCoinKey: string;
	lendAmount: number;
	poolKey: string;
	clientOrderId: string; // numeric string, encoded as u64
	price: number;
	quantity: number;
	stakeAmount: number;
}

export function lendTradeStake(
	client: DeepBookMarginClient,
	p: LendTradeStakeParams,
): Transaction {
	const tx = new Transaction();
	// Leg 1 (lend): supply to the margin pool with the reusable SupplierCap.
	tx.add(
		client.deepbook.marginPool.supplyToMarginPool(p.lendCoinKey, tx.object(p.supplierCapId), p.lendAmount),
	);
	// Leg 2 (trade): rest a maker order. POST_ONLY so it never crosses and takes.
	tx.add(
		client.deepbook.deepBook.placeLimitOrder({
			poolKey: p.poolKey,
			balanceManagerKey: p.balanceManagerKey,
			clientOrderId: p.clientOrderId,
			price: p.price,
			quantity: p.quantity,
			isBid: false,
			orderType: OrderType.POST_ONLY,
		}),
	);
	// Leg 3 (stake): stake DEEP into the pool's governance for fee rebates.
	tx.add(client.deepbook.governance.stake(p.poolKey, p.balanceManagerKey, p.stakeAmount));
	return tx;
}
// docs::/#lend-trade-stake
