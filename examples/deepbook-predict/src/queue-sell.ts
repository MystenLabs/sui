// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#queue-sell
import type {
	MarketCoordinates,
	QueuedOrderPreview,
	SellPlan,
} from '@mysten/deepbook-v3/predict';
import { cost } from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';

// Sell part or all of an Open record before expiry. The sell is a queued order
// too: it fills at the price for its own instant, within the floors below, or
// it is refunded and the position goes back to an Open record.
export async function queueSell(params: {
	owner: string;
	market: MarketCoordinates;
	// The Open record's ID from the mint's enqueue receipt. After a sell, the
	// position lives under the sell's record ID instead.
	recordId: bigint;
	// Payout quantity to sell, in USD. Must be a whole $0.01 lot.
	quantity: number;
	slippageCents: number;
}): Promise<{ tx: Transaction; plan: SellPlan; preview: QueuedOrderPreview }> {
	const { owner, market, recordId, quantity, slippageCents } = params;

	// `net` is the proceeds less the flat order fee, which the sell pays at
	// enqueue whatever it returns, so it is negative for a sell worth less than
	// the fee.
	const plan = await client.predict.read.planSell(owner, market, {
		recordId,
		quantity,
		slippageCents,
	});
	if (!plan.accepting) {
		throw new Error(`The market refuses this sell now: ${plan.refusal}.`);
	}

	// When the sell could need more cash than the market holds spare, the
	// builder appends `rebalance_expiry_cash` so the market is funded before the
	// fill (`preview.fundedInTransaction`).
	const { transaction, preview } = await client.predict.tx.enqueuePlan(owner, market, plan);
	return { tx: transaction, plan, preview };
}

// Keepers refund an unfinished order at its deadline, a few seconds after its
// price instant. Offer a manual refund only once the record says it is due.
export async function refundIfOverdue(
	market: MarketCoordinates,
	recordId: bigint,
): Promise<Transaction | null> {
	const order = await client.predict.read.order(market, recordId);
	const view = order?.view;
	const due =
		view !== undefined &&
		(view.state === 'placed' || view.state === 'priced' || view.state === 'refunding') &&
		view.canRequestRefund;
	return due ? client.predict.tx.refund(market) : null;
}

// At expiry a keeper settles the market, pays every Open record to its
// account, and deletes the records, usually within seconds. Keep each filled
// record's `position.orderId` to show the result afterwards. This computes
// what the position is owed at the settlement price. The payment itself is the
// `OpenRecordSettled` event, and a record the payout walk skipped emits
// `OpenRecordPayoutSkipped` and waits for `tx.payOpen`.
export async function settledPayout(
	market: MarketCoordinates,
	orderId: bigint,
): Promise<bigint | null> {
	const settlement = await client.predict.read.settlement(market);
	if (!settlement.settled || settlement.settlementPriceRaw === null) {
		return null;
	}
	// Raw USDC owed at the settlement price. Zero for a losing position.
	return cost.settledPayout(orderId, settlement.settlementPriceRaw, settlement.tickSizeRaw);
}
// docs::/#queue-sell
