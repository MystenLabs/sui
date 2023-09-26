// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EventId } from '@mysten/sui.js/src/client';

export * from './bcs';

export interface PoolSummary {
	poolId: string;
	baseAsset: string;
	quoteAsset: string;
}

/**
 * `next_cursor` points to the last item in the page; Reading with `next_cursor` will start from the
 * next item after `next_cursor` if `next_cursor` is `Some`, otherwise it will start from the first
 * item.
 */
export interface PaginatedPoolSummary {
	data: PoolSummary[];
	hasNextPage: boolean;
	nextCursor?: EventId | null;
}

export interface UserPosition {
	availableBaseAmount: bigint;
	lockedBaseAmount: bigint;
	availableQuoteAmount: bigint;
	lockedQuoteAmount: bigint;
}

export enum LimitOrderType {
	// Fill as much quantity as possible in the current transaction as taker, and inject the remaining as a maker order.
	NO_RESTRICTION = 0,
	// Fill as much quantity as possible in the current transaction as taker, and cancel the rest of the order.
	IMMEDIATE_OR_CANCEL = 1,
	// Only fill if the entire order size can be filled as taker in the current transaction. Otherwise, abort the entire transaction.
	FILL_OR_KILL = 2,
	// Only proceed if the entire order size can be posted to the order book as maker in the current transaction. Otherwise, abort the entire transaction.
	POST_OR_ABORT = 3,
}

// The self-matching prevention mechanism ensures that the matching engine takes measures to avoid unnecessary trades
// when matching a user's buy/sell order with their own sell/buy order.
// NOTE: we have only implemented one variant for now
export enum SelfMatchingPreventionStyle {
	// Cancel older (resting) order in full. Continue to execute the newer taking order.
	CANCEL_OLDEST = 0,
}

export interface Order {
	orderId: string;
	clientOrderId: string;
	price: string;
	originalQuantity: string;
	quantity: string;
	isBid: boolean;
	owner: string;
	expireTimestamp: string;
	selfMatchingPrevention: SelfMatchingPreventionStyle;
}

export interface MarketPrice {
	bestBidPrice: bigint | undefined;
	bestAskPrice: bigint | undefined;
}

export interface Level2BookStatusPoint {
	price: bigint;
	depth: bigint;
}
