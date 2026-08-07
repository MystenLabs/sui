// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#refresh-prices
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// Refresh the Pyth price feeds a margin transaction depends on, in that same
// transaction. Every pool proxy order, and every call that values a position
// (borrow, withdraw, risk read), reads the pool's `PriceInfoObject` for both the
// base and the quote coin and reverts if the onchain price is older than the
// pool's maximum age.
//
// `getPriceInfoObjects` handles both cases: when the onchain price is still
// fresh it returns the existing object IDs and adds nothing, and when it is
// stale it fetches a signed update from Pyth's Hermes endpoint and appends the
// verify-and-update commands to `tx` before your own commands run. Because it
// fetches over the network it is async, so call it before you add the margin
// command, and use the same `tx` for both.
export async function refreshPriceFeeds(
	client: DeepBookMarginClient,
	tx: Transaction,
	coinKeys: string[],
): Promise<Record<string, string>> {
	return client.deepbook.getPriceInfoObjects(tx, coinKeys);
}
// docs::/#refresh-prices

// docs::#order-with-fresh-prices
// Place a margin order with the price feeds refreshed in the same transaction.
// Refreshing first and ordering second keeps the two atomic: the order reads the
// price this transaction just wrote, so it cannot revert on an age check that
// passed when you built the transaction but failed by the time it executed.
export async function openLongWithFreshPrices(
	client: DeepBookMarginClient,
	marginManagerKey: string,
	poolKey: string,
	baseCoinKey: string,
	quoteCoinKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Promise<Transaction> {
	const tx = new Transaction();
	await client.deepbook.getPriceInfoObjects(tx, [baseCoinKey, quoteCoinKey]);
	tx.add(
		client.deepbook.poolProxy.placeLimitOrder({
			poolKey,
			marginManagerKey,
			clientOrderId,
			price,
			quantity,
			isBid: true,
		}),
	);
	return tx;
}
// docs::/#order-with-fresh-prices

// docs::#price-age
// Read how old a coin's onchain Pyth price is, in seconds. Use it to decide
// whether a read-only call (such as `getMarginManagerState`) is about to revert
// on a stale price, and to alert before a keeper falls behind.
export async function priceAgeSeconds(
	client: DeepBookMarginClient,
	coinKey: string,
): Promise<number> {
	return client.deepbook.getPriceInfoObjectAge(coinKey);
}
// docs::/#price-age
