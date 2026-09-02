// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#markets
import type { ActiveMarket, MarketSummary } from '@mysten/deepbook-v3/predict';
import { client } from './client.js';
import { UNDERLYING } from './config.js';

// Markets are created on a fixed cadence and every expiry is an absolute
// timestamp, so never hardcode one. Read the live board and take an expiry from
// it: `read.markets()` returns only markets that are currently tradeable.
export async function liveMarkets(): Promise<ActiveMarket[]> {
	const markets = await client.predict.read.markets();
	// `expiryMs` is a bigint, so order it by comparison rather than subtraction.
	return [...markets].sort((a, b) =>
		a.expiryMs < b.expiryMs ? -1 : a.expiryMs > b.expiryMs ? 1 : 0,
	);
}

// The soonest expiry that still accepts mints and has its window anchor seeded.
// `referencePrice` is null for a short time at the start of a window, until the
// keeper seeds it.
export async function nearestOpenMarket(): Promise<ActiveMarket> {
	const open = (await liveMarkets()).filter((m) => !m.mintPaused && m.referencePrice !== null);
	if (open.length === 0) {
		throw new Error('No DeepBook Predict market is open for minting right now.');
	}
	return open[0];
}

// One market's on-chain state, including its live NAV. Returns null when no
// market exists at that expiry.
export async function marketState(expiryMs: bigint): Promise<MarketSummary | null> {
	return client.predict.read.market({ underlying: UNDERLYING, expiryMs });
}

// A numeric strike must be a whole multiple of the market's `admissionTickSize`,
// which is deliberately coarser than `tickSize` and varies by cadence. Round the
// target onto that grid rather than assuming a step: an off-grid strike throws
// `PredictInputError` when the transaction is built. The market's own
// `referencePrice` is the single finite strike the chain admits off-grid.
export function admissibleStrike(market: ActiveMarket, targetUsd: number): number {
	return Math.round(targetUsd / market.admissionTickSize) * market.admissionTickSize;
}
// docs::/#markets
