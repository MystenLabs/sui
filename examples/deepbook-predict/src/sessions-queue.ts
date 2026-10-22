// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#sessions-queue
import type { MarketDescriptor, Side } from '@mysten/deepbook-v3/predict';
import { binaryRangeTicks, priceToRaw, snapStrike } from '@mysten/deepbook-v3/predict';
import { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { UNDERLYING } from './config.js';
import { tradeableMarket } from './markets.js';
import { sessions, wrapperId } from './sessions-authorize.js';

// The queued session wrappers place an order in the market's queue as the
// session key. They need Sessions protocol version 3 and a network past the
// delayed-execution cutover. Besides the market and the account, each takes
// the order-flow package's `OrderDesk` and `QueueRegistry`, which derives the
// market's queue ID, and the oracle objects the order snapshots its pricing
// inputs from. No pricer: placement reads the oracles itself.
function queueTarget(expiryMarketId: string, owner: string) {
	const { objects, underlyings } = client.predict.cfg;
	if (!objects.orderDesk || !objects.queueRegistry) {
		throw new Error('This network has no delayed-execution deployment recorded.');
	}
	return {
		expiryMarketId,
		wrapperId: wrapperId(owner),
		orderDesk: objects.orderDesk,
		queueRegistry: objects.queueRegistry,
		protocolConfig: objects.protocolConfig,
		oracleRegistry: objects.oracleRegistry,
		pythFeed: underlyings[UNDERLYING].pythFeed,
		blockScholesValueStore: underlyings[UNDERLYING].blockScholesValueStore,
		blockScholesSviStore: underlyings[UNDERLYING].blockScholesSviStore,
	};
}

// Queue an all-in budget mint as the session key. A session trades the owner's
// account, so plan against the owner and send the plan's raw limits: the
// wrapper skips the SDK's preflight, so check `accepting` first.
export async function queueMintAsSession(params: {
	owner: string;
	session: string;
	side: Side;
	amount: number;
	slippageCents: number;
}): Promise<Transaction> {
	const { owner, session, side, amount, slippageCents } = params;
	const market = await tradeableMarket();
	const pricer = await client.predict.read.pricer({
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
	});
	const atTheMoney = pricer.strikeAtProbability(0.5);
	if (atTheMoney === null) {
		throw new Error(`Market ${market.id} has no strike near 50 cents.`);
	}
	const strike = snapStrike(atTheMoney, market.admissionTickSize);

	const descriptor: MarketDescriptor = {
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
		marketId: market.id,
		side,
		strike,
	};
	const plan = await client.predict.read.planMint(owner, descriptor, { amount, slippageCents });
	if (!plan.accepting) {
		throw new Error(`The market refuses this order now: ${plan.refusal}.`);
	}

	const { lowerTick, higherTick } = binaryRangeTicks(
		priceToRaw(strike),
		side,
		priceToRaw(market.tickSize),
	);

	const tx = new Transaction();
	tx.setSender(session);
	tx.add(
		sessions.enqueueExactCost({
			...queueTarget(market.id, owner),
			lowerTick,
			higherTick,
			// The escrowed all-in budget. A queued mint needs a finite cap: zero and
			// U64_MAX both abort `EMintCostCapRequired`.
			maxCost: plan.raw.budget,
			// A fill below this payout quantity is refunded rather than filled.
			minQuantity: plan.raw.minQuantity,
		}),
	);
	return tx;
}

// Queue an early sell of an Open record as the session key. The floors are
// required: on a delegated key, zero sells at any price.
export async function queueSellAsSession(params: {
	owner: string;
	session: string;
	expiryMarketId: string;
	expiryMs: bigint;
	recordId: bigint;
	quantity: number;
	slippageCents: number;
}): Promise<Transaction> {
	const { owner, session, expiryMarketId, expiryMs, recordId, quantity, slippageCents } = params;
	const plan = await client.predict.read.planSell(
		owner,
		{ underlying: UNDERLYING, expiryMs, marketId: expiryMarketId },
		{ recordId, quantity, slippageCents },
	);
	if (!plan.accepting) {
		throw new Error(`The market refuses this sell now: ${plan.refusal}.`);
	}

	const tx = new Transaction();
	tx.setSender(session);
	tx.add(
		sessions.enqueueRedeemOpen({
			...queueTarget(expiryMarketId, owner),
			recordId,
			closeQuantity: plan.quote.raw.quantityClosed,
			minProbability: plan.raw.minProbability,
			minProceeds: plan.raw.minProceeds,
		}),
	);
	return tx;
}
// docs::/#sessions-queue
