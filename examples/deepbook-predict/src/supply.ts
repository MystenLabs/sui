// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#supply
import type {
	DecodableTransactionResult,
	PlpRequestReceipt,
	PoolSummary,
} from '@mysten/deepbook-v3/predict';
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';

// Supplying to the pool queues a request rather than minting PLP on the spot.
// This transaction returns no PLP: the request fills at the next pool flush, at
// the single NAV that flush computes. `minPlpOut` is a floor on that mark, in raw
// six-decimal shares: a flush quoting fewer shares declines rather than filling
// smaller, and at the deployed attempt count of one the first miss cancels and
// refunds the request. Omit it to accept whatever the next flush quotes.
export function queueSupply(
	owner: string,
	amountUsdc: number,
	minPlpOut?: bigint,
): Transaction {
	return client.predict.tx.supplyPlp(owner, amountUsdc, { minPlpOut });
}

// The queue index is the handle for cancelling a request before it fills, and it
// exists only in the receipt. Execute with events included and keep it.
export function supplyRequestIndex(result: DecodableTransactionResult): bigint {
	// `kind` is 'supply' here, and `amount` is in quote units.
	const receipt: PlpRequestReceipt = client.predict.decode.plpRequest(result);
	return receipt.index;
}

// Pool state. `supplyRequestsPending` and `withdrawRequestsPending` are queue
// lengths rather than amounts, and `plpTotalSupply` is raw six-decimal shares.
export async function poolState(): Promise<PoolSummary> {
	return client.predict.read.pool();
}
// docs::/#supply
