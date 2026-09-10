// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#mint-range
import type { MarketDescriptor, MintQuote } from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { UNDERLYING } from './config.js';
import { admissibleStrike, tradeableMarket } from './markets.js';

// A range position pays out when settlement lands inside `(lower, upper]`, which
// is left-open and right-closed. Both bounds are finite numeric strikes, so both
// must sit on the market's admission grid. There is no `strike` field on the
// range arm of the descriptor, so `'reference'` has no meaning here: center the
// band on the market's reference price instead.
export async function mintRange(params: {
	owner: string;
	// Maximum payout in USD, at $1 per contract. Must be a whole $0.01 lot.
	quantity: number;
	// Half-width of the band around the window anchor, in USD.
	halfWidthUsd: number;
}): Promise<{ tx: Transaction; quote: MintQuote; lower: number; upper: number }> {
	const { owner, quantity, halfWidthUsd } = params;
	const market = await tradeableMarket();

	const anchor = market.referencePrice;
	if (anchor === null) {
		throw new Error('The market has no reference price yet. Retry once the keeper seeds it.');
	}

	const lower = admissibleStrike(market, anchor - halfWidthUsd);
	// Each bound rounds independently, so a band narrower than one admission tick
	// can collapse onto a single tick. The chain requires `lower` strictly below
	// `upper`.
	const upper = Math.max(
		admissibleStrike(market, anchor + halfWidthUsd),
		lower + market.admissionTickSize,
	);

	const descriptor: MarketDescriptor = {
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
		marketId: market.id,
		side: 'range',
		lower,
		upper,
	};

	// Range positions quote and mint through the same builders as binary ones.
	const quote = await client.predict.read.quoteMint(owner, descriptor, { quantity });

	// Cap both dimensions. Omitting either sends U64_MAX for it, and a cost cap
	// alone still lets the fill price move: pass `maxProbability` as well.
	const maxCost = Math.ceil(quote.cost * 1.01 * 1e6) / 1e6;
	const maxProbability = Math.min(1, Number((quote.entryProbability * 1.02).toFixed(6)));

	const tx = await client.predict.tx.mint(owner, descriptor, {
		quantity,
		maxCost,
		maxProbability,
	});

	return { tx, quote, lower, upper };
}
// docs::/#mint-range
