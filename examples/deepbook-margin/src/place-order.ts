// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// Margin orders go through the pool proxy, not the spot order entry, so the
// borrow and the trade stay bound to the MarginManager. `clientOrderId` is your
// own tag: the SDK types it as a string, but the contract encodes it as u64, so
// pass a numeric string such as a timestamp, not a label. Price and quantity
// must respect the pool's tick and lot size.

// docs::#open-position
// Open the leveraged position. With borrowed DBUSDC in the manager, place a bid
// on SUI_DBUSDC to buy SUI: your position is now larger than your own collateral
// funded, which is the leverage.
export function openLongPosition(
	client: DeepBookMarginClient,
	marginManagerKey: string,
	poolKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Transaction {
	const tx = new Transaction();
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
// docs::/#open-position

// docs::#reduce-position
// Close or shrink the position with a reduce-only order. Reduce-only guarantees
// the order can only decrease your exposure, never flip you to the other side or
// add leverage: here it sells SUI back for DBUSDC so you can repay the borrow.
export function reduceLongPosition(
	client: DeepBookMarginClient,
	marginManagerKey: string,
	poolKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Transaction {
	const tx = new Transaction();
	tx.add(
		client.deepbook.poolProxy.placeReduceOnlyLimitOrder({
			poolKey,
			marginManagerKey,
			clientOrderId,
			price,
			quantity,
			isBid: false,
		}),
	);
	return tx;
}
// docs::/#reduce-position
