// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { OrderType, SelfMatchingOptions } from '@mysten/deepbook-v3';
import type { DeepBookTestnetClient } from './client.js';

// `clientOrderId` is your own tag for the order. The SDK types it as a string,
// but the contract encodes it as u64 (`tx.pure.u64`), so it MUST be a numeric
// string such as a timestamp. A non-numeric label like 'maker-1' throws
// "Cannot convert maker-1 to a BigInt" at build time. Keep human labels in your
// logs, not here.

// docs::#place-maker
// Place a resting maker order. POST_ONLY guarantees it rests as liquidity: if it
// would cross the book it is not placed, so you never pay a taker fee by
// accident. `price` and `quantity` must respect the pool's tick and lot size.
export function placeMakerAsk(
	client: DeepBookTestnetClient,
	managerKey: string,
	poolKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Transaction {
	const tx = new Transaction();
	tx.add(
		client.deepbook.deepBook.placeLimitOrder({
			poolKey,
			balanceManagerKey: managerKey,
			clientOrderId,
			price,
			quantity,
			isBid: false,
			orderType: OrderType.POST_ONLY,
		}),
	);
	return tx;
}
// docs::/#place-maker

// docs::#place-taker
// Place a taker order that crosses the book and fills. `selfMatchingOption` is
// set EXPLICITLY here: the SDK defaults to SELF_MATCHING_ALLOWED, so a taker
// order can fill your OWN resting maker order without any warning. Set
// CANCEL_TAKER or CANCEL_MAKER to refuse trading against yourself. This workflow
// allows it on purpose, to fill the maker order placed above and observe a fill.
export function placeTakerBid(
	client: DeepBookTestnetClient,
	managerKey: string,
	poolKey: string,
	clientOrderId: string, // numeric string, encoded as u64
	price: number,
	quantity: number,
): Transaction {
	const tx = new Transaction();
	tx.add(
		client.deepbook.deepBook.placeLimitOrder({
			poolKey,
			balanceManagerKey: managerKey,
			clientOrderId,
			price,
			quantity,
			isBid: true,
			orderType: OrderType.NO_RESTRICTION,
			selfMatchingOption: SelfMatchingOptions.SELF_MATCHING_ALLOWED,
		}),
	);
	return tx;
}
// docs::/#place-taker
