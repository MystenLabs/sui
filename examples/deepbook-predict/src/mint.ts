// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#mint-binary
import type { MarketDescriptor, MintQuote } from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { UNDERLYING } from './config.js';
import { admissibleStrike, nearestOpenMarket } from './markets.js';

// Quote, then mint with a cap derived from the quote.
//
// Both caps are optional, and omitting them is not a safe default: the SDK sends
// U64_MAX for a missing `maxCost` or `maxProbability`, which leaves the mint
// genuinely uncapped. If the price moves between the quote and execution, an
// uncapped mint can debit the account's entire balance. Always pass at least
// `maxCost`.
export async function mintDirectional(params: {
	owner: string;
	side: 'up' | 'down';
	// Maximum payout in USD, at $1 per contract. Must be a whole $0.01 lot.
	quantity: number;
	// Omit to trade at the market's on-chain reference price, the window anchor.
	targetStrikeUsd?: number;
}): Promise<{ tx: Transaction; quote: MintQuote; descriptor: MarketDescriptor }> {
	const { owner, side, quantity, targetStrikeUsd } = params;
	const market = await nearestOpenMarket();

	const descriptor: MarketDescriptor = {
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
		// Pin the exact market object that was read, rather than whatever the
		// registry resolves to at submit time.
		marketId: market.id,
		side,
		strike:
			targetStrikeUsd === undefined
				? 'reference'
				: admissibleStrike(market, targetStrikeUsd),
	};

	// The quote dry-runs the identical transaction the mint builds, against the
	// real account and the real fee path, so it doubles as preflight: it throws
	// the same typed errors the mint would.
	const quote = await client.predict.read.quoteMint(owner, descriptor, { quantity });

	// `quote.cost` is the all-in account debit, not the premium. Raw amounts are
	// integers at six decimals, so round the cap to six decimals: a finer value
	// throws `PredictInputError`.
	const maxCost = Math.ceil(quote.cost * 1.01 * 1e6) / 1e6;

	// A second, independent ceiling on the fill price, 0..1 per $1 of payout.
	const maxProbability = Math.min(1, Number((quote.entryProbability * 1.02).toFixed(6)));

	const tx = await client.predict.tx.mint(owner, descriptor, {
		quantity,
		maxCost,
		maxProbability,
	});

	// The transaction is ready to sign. Nothing here signs it, and the SDK never
	// holds keys.
	return { tx, quote, descriptor };
}
// docs::/#mint-binary
