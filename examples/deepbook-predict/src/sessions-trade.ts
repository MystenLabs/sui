// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#sessions-trade
import type { MarketDescriptor, MintQuote, Side } from '@mysten/deepbook-v3/predict';
import {
	binaryRangeTicks,
	getConfig,
	loadLivePricer,
	priceToRaw,
	probabilityToRaw,
	toGeneratedConfig,
	usdcToRaw,
} from '@mysten/deepbook-v3/predict';
import { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { NETWORK, UNDERLYING } from './config.js';
import { admissibleStrike, tradeableMarket } from './markets.js';
import { SESSIONS_CONFIG, sessions, wrapperId } from './sessions-authorize.js';

// The Predict session wrappers take the same market parameters as Predict itself,
// so they need the Predict deployment's IDs alongside the sessions IDs. The
// `/predict` subpath exports the projection and the pricer loader precisely so
// these wrappers can be composed.
const PREDICT = getConfig(NETWORK);
const GENERATED = toGeneratedConfig(PREDICT);
const FEEDS = PREDICT.underlyings[UNDERLYING];

// Mint a directional position as the session key.
//
// Three things differ from the owner-signed `/predict` flow:
//
// 1. The session key is the sender. The wrapper derives the caller from the
//    sender, mints app authorization internally, and consumes it in the same
//    call, so no `Auth` value is built or passed.
// 2. `pricer` is a PTB result, not an object ID. Load it with
//    `loadLivePricer` in a preceding command of the same transaction.
// 3. `maxCost` and `maxProbability` are required. The `/predict` facade defaults
//    a missing cap to `U64_MAX`, which is no cap at all; the session builder
//    makes you name both. Quantities and costs are raw six-decimal DUSDC, and
//    probabilities are raw fixed point at 1e9.
export async function mintAsSession(params: {
	owner: string;
	session: string;
	side: Side;
	// Maximum payout in USD, at $1 per contract. Must be a whole $0.01 lot.
	quantity: number;
	// Omit to trade at the market's onchain reference price, the window anchor.
	targetStrikeUsd?: number;
}): Promise<{ tx: Transaction; quote: MintQuote; descriptor: MarketDescriptor }> {
	const { owner, session, side, quantity, targetStrikeUsd } = params;
	const market = await tradeableMarket();

	// The session wrappers take absolute ticks, so resolve the strike to a number
	// before converting. `tradeableMarket` already skips markets whose reference
	// price is unseeded.
	if (market.referencePrice === null) {
		throw new Error(`Market ${market.id} has no reference price yet.`);
	}
	const strikeUsd =
		targetStrikeUsd === undefined
			? market.referencePrice
			: admissibleStrike(market, targetStrikeUsd);

	const { lowerTick, higherTick } = binaryRangeTicks(
		priceToRaw(strikeUsd),
		side,
		priceToRaw(market.tickSize),
	);

	// Quote through the owner path first. The quote dry-runs the same market, the
	// same account, and the same fee path, so its `cost` is the figure to size the
	// cap against. Quote immediately before sending: on the short cadences the
	// entry probability moves fast.
	const descriptor: MarketDescriptor = {
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
		marketId: market.id,
		side,
		strike: strikeUsd,
	};
	const quote = await client.predict.read.quoteMint(owner, descriptor, { quantity });

	const tx = new Transaction();
	// The session key signs, not the owner.
	tx.setSender(session);

	const pricer = tx.add(
		loadLivePricer(GENERATED, {
			expiryMarketId: market.id,
			pythFeed: FEEDS.pythFeed,
			blockScholesValueStore: FEEDS.blockScholesValueStore,
			blockScholesSviStore: FEEDS.blockScholesSviStore,
		}),
	);

	tx.add(
		sessions.mintExactQuantity({
			expiryMarketId: market.id,
			wrapperId: wrapperId(owner),
			protocolConfig: SESSIONS_CONFIG.protocolConfig,
			pricer,
			lowerTick,
			higherTick,
			quantity: usdcToRaw(quantity),
			// All-in debit ceiling, one percent above the quoted cost.
			maxCost: usdcToRaw(Math.ceil(quote.cost * 1.01 * 1e6) / 1e6),
			// Independent ceiling on the fill price, 0 to 1 per $1 of payout.
			maxProbability: probabilityToRaw(
				Math.min(1, Number((quote.entryProbability * 1.02).toFixed(9))),
			),
		}),
	);

	return { tx, quote, descriptor };
}

// Close part or all of a live position as the session key.
//
// `minProbability` and `minProceeds` are close-side floors, and omitting either
// sends zero. Zero on a delegated key means the position closes at whatever the
// mark gives, so pass real floors on anything a session key can reach. The
// `/predict` facade sends zero for both and exposes no way to raise them, which
// is why the session builder is the one that can protect a close.
export async function closeAsSession(params: {
	owner: string;
	session: string;
	expiryMarketId: string;
	orderId: bigint;
	// Payout quantity to close, in USD. Must be a whole $0.01 lot.
	quantity: number;
	// Minimum acceptable fill probability, 0 to 1.
	minProbability: number;
	// Minimum acceptable DUSDC proceeds for the whole close.
	minProceeds: number;
}): Promise<Transaction> {
	const { owner, session, expiryMarketId, orderId, quantity } = params;

	const tx = new Transaction();
	tx.setSender(session);

	const pricer = tx.add(
		loadLivePricer(GENERATED, {
			expiryMarketId,
			pythFeed: FEEDS.pythFeed,
			blockScholesValueStore: FEEDS.blockScholesValueStore,
			blockScholesSviStore: FEEDS.blockScholesSviStore,
		}),
	);

	tx.add(
		sessions.redeemLive({
			expiryMarketId,
			wrapperId: wrapperId(owner),
			protocolConfig: SESSIONS_CONFIG.protocolConfig,
			pricer,
			orderId,
			closeQuantity: usdcToRaw(quantity),
			minProbability: probabilityToRaw(params.minProbability),
			minProceeds: usdcToRaw(params.minProceeds),
		}),
	);

	// A partial close retires the order ID and returns a replacement as
	// `Option<u256>`. Read it from the transaction result and store it, or the
	// next close targets an order that no longer exists.
	return tx;
}

// Claim a settled position as the session key. No pricer, because the settlement
// price is fixed, and no quantity, because a settled claim closes the order in
// full.
export function claimSettledAsSession(params: {
	owner: string;
	session: string;
	expiryMarketId: string;
	orderId: bigint;
}): Transaction {
	const { owner, session, expiryMarketId, orderId } = params;

	const tx = new Transaction();
	tx.setSender(session);
	tx.add(
		sessions.redeemSettled({
			expiryMarketId,
			wrapperId: wrapperId(owner),
			protocolConfig: SESSIONS_CONFIG.protocolConfig,
			orderId,
		}),
	);
	return tx;
}
// docs::/#sessions-trade
