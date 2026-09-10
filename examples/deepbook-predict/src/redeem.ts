// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#redeem
import type {
	CloseOptions,
	DecodableTransactionResult,
	MarketDescriptor,
	OpenPosition,
	RedeemQuote,
	RedeemReceipt,
} from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { UNDERLYING } from './config.js';

// Close part or all of a live position. Reuse the descriptor the position was
// minted with: the close needs it only to resolve the market object, and the
// order ID identifies the position itself.
//
// The quote is the only protection available on a live close. `tx.redeem` sends
// `minProbability: 0` and `minProceeds: 0` unconditionally, and the facade
// exposes no option to raise either floor, so the proceeds are not capped
// against a price move between the quote and execution. Quote immediately before
// closing and treat the figure as an estimate.
export async function closeLive(
	owner: string,
	descriptor: MarketDescriptor,
	opts: CloseOptions,
): Promise<{ tx: Transaction; quote: RedeemQuote }> {
	const quote = await client.predict.read.quoteRedeem(owner, descriptor, opts);
	const tx = await client.predict.tx.redeem(owner, descriptor, opts);
	return { tx, quote };
}

// A partial close retires the old order ID and issues a new one, reported as
// `replacementOrderId`. It is null when the position closed in full. Store the
// replacement, or the next close targets an order that no longer exists.
export function decodeClose(result: DecodableTransactionResult): RedeemReceipt {
	return client.predict.decode.redeem(result);
}

// Claim a position whose market has settled. The claim closes the order in full,
// so it takes no quantity, and it needs only the market coordinates rather than
// a side or a strike.
export async function claimSettled(params: {
	owner: string;
	expiryMs: bigint;
	marketId: string;
	orderId: bigint;
}): Promise<Transaction> {
	const { owner, expiryMs, marketId, orderId } = params;
	return client.predict.tx.claimSettled(
		owner,
		{ underlying: UNDERLYING, expiryMs, marketId },
		{ orderId },
	);
}

// Every open position for an owner, read straight from the account's on-chain
// position table. Use it to recover order IDs from a cold start.
export async function openPositions(owner: string): Promise<OpenPosition[]> {
	return client.predict.read.positions(owner);
}
// docs::/#redeem
