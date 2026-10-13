// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#queue-mint
import type {
	DecodableTransactionResult,
	MarketDescriptor,
	MintPlan,
	QueuedOrderOutcome,
	QueuedOrderPreview,
} from '@mysten/deepbook-v3/predict';
import { snapStrike } from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { UNDERLYING } from './config.js';
import { tradeableMarket } from './markets.js';

// From the delayed-execution cutover, a mint is a queued order. It is placed
// now, priced at Pyth's signed price for an instant shortly after placement,
// and then filled or refunded by a later transaction that anyone can send.
// `executionMode` reports which path a network is on, so an app can switch at
// the cutover without a redeploy.
export async function assertQueuedOrders(): Promise<void> {
	const mode = await client.predict.read.executionMode();
	if (mode !== 'delayed') {
		throw new Error(`This network trades in '${mode}' mode, not 'delayed'.`);
	}
}

// Plan, then queue, a directional mint that spends `amount` USDC all in.
export async function queueMint(params: {
	owner: string;
	side: 'up' | 'down';
	// The whole debit in USDC, order fee included.
	amount: number;
	// How far the all-in price per contract may rise before the fill, in cents,
	// above the quote's price without the fee subsidy. A contract pays $1, so 10
	// lets a contract quoted at 42 cents before the subsidy fill at up to 52.
	slippageCents: number;
}): Promise<{
	tx: Transaction;
	plan: MintPlan;
	preview: QueuedOrderPreview;
	descriptor: MarketDescriptor;
}> {
	const { owner, side, amount, slippageCents } = params;
	const market = await tradeableMarket();

	// Admission refuses an entry probability outside the market's band, and the
	// reference strike drifts away from the money as the price moves, so pick
	// the strike that prices near 50 cents and snap it onto the admission grid.
	const pricer = await client.predict.read.pricer({
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
	});
	const atTheMoney = pricer.strikeAtProbability(0.5);
	if (atTheMoney === null) {
		throw new Error(`Market ${market.id} has no strike near 50 cents.`);
	}

	const descriptor: MarketDescriptor = {
		underlying: UNDERLYING,
		expiryMs: market.expiryMs,
		marketId: market.id,
		side,
		strike: snapStrike(atTheMoney, market.admissionTickSize),
	};

	// The plan quotes the order at the current price and turns the slippage into
	// the limits the order carries onchain. It also runs the enqueue builder's
	// preflight of the queue gates, the balance, and the limits, so `accepting`
	// is false, with a typed `refusal`, when the order would be refused now. A
	// few checks run only onchain at placement, so a plan that accepts can still
	// abort there.
	const plan = await client.predict.read.planMint(owner, descriptor, {
		amount,
		slippageCents,
	});
	if (!plan.accepting) {
		throw new Error(`The market refuses this order now: ${plan.refusal}.`);
	}

	// `preview.totalDebit` is the budget plus the flat order fee, which leave
	// the account at enqueue. Unused budget comes back when the order fills.
	const { transaction, preview } = await client.predict.tx.enqueuePlan(owner, descriptor, plan);
	return { tx: transaction, plan, preview, descriptor };
}

// After the enqueue executes, with events included, its record ID is the
// handle for everything else: following the order, selling the position, and
// showing the result. Persist it.
export async function awaitQueuedOrder(
	descriptor: MarketDescriptor,
	result: DecodableTransactionResult,
): Promise<{ recordId: bigint; outcome: QueuedOrderOutcome }> {
	const { recordId } = client.predict.decode.enqueue(result);

	// Polls until the record is filled or refunded, about a second after
	// placement. A filled mint stays in the market's queue as an Open record,
	// so `outcome.order.view.position` holds the position, not the account.
	const outcome = await client.predict.read.waitForOutcome(descriptor, recordId);
	return { recordId, outcome };
}
// docs::/#queue-mint
