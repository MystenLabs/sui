// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#markets
import type { ActiveMarket, MarketSummary } from '@mysten/deepbook-v3/predict';
import { client } from './client.js';
import { UNDERLYING } from './config.js';

// Markets are created on a fixed cadence and every expiry is an absolute
// timestamp, so never hardcode one. Read the live board and take an expiry from
// it. `read.markets()` returns the pool's active markets, which means live and
// not yet settled: settlement is permissionless and unrewarded, so a market that
// is past its expiry but that nobody has settled is still in this list, and
// quoting against it aborts.
export async function liveMarkets(): Promise<ActiveMarket[]> {
	const markets = await client.predict.read.markets();
	// `expiryMs` is a bigint, so order it by comparison rather than subtraction.
	return [...markets].sort((a, b) =>
		a.expiryMs < b.expiryMs ? -1 : a.expiryMs > b.expiryMs ? 1 : 0,
	);
}

// A market with enough life left to quote, sign, and land a transaction.
//
// Taking the soonest expiry is a trap. On the one-minute cadence the entry
// probability converges toward 0 or 1 in the closing seconds, so a quote taken
// there is stale before the mint executes and the `maxCost` cap then aborts the
// trade. Requiring a minimum time to expiry is what makes the quote-then-cap
// flow hold. Raise `minTtlMs` for a wallet flow that waits on a human.
//
// `referencePrice` is null for a short time at the start of a window. Anyone can
// seed it with the permissionless `expiry_market::set_reference_tick`; this
// helper skips those markets instead.
export async function tradeableMarket(minTtlMs = 30_000): Promise<ActiveMarket> {
	const now = Date.now();
	const open = (await liveMarkets()).filter(
		(m) =>
			!m.mintPaused &&
			m.referencePrice !== null &&
			Number(m.expiryMs) - now >= minTtlMs,
	);
	if (open.length === 0) {
		throw new Error(
			`No DeepBook Predict market has ${minTtlMs} ms or more left before expiry.`,
		);
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
	const snapped = Math.round(targetUsd / market.admissionTickSize) * market.admissionTickSize;
	// Trim binary-float residue before returning. Strikes scale by 1e9 and the SDK
	// throws when a value carries more than nine decimals, which the multiply above
	// produces for sub-dollar steps: at a 0.1 step it lands on values such as
	// 96519.90000000001. Today's cadences use 1 and 100, so this is defensive, but
	// the step is mutable protocol state and is read from the market.
	return Number(snapped.toFixed(9));
}
// docs::/#markets
