// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#withdraw
import type { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';

// Withdrawing from the pool is queued exactly as a supply is, and it takes raw
// PLP shares rather than a USD amount. `read.plpBalance` returns those shares
// directly, so exiting the whole position needs no conversion. The builder pins
// the minimum DUSDC out to zero, so there is no per-request floor to set.
export async function queueWithdrawAll(owner: string): Promise<Transaction> {
	const shares = await client.predict.read.plpBalance(owner);
	// The chain rejects a request below one whole PLP, which is 1_000_000 raw at
	// six decimals. Check it here rather than letting a dust holder take a Move
	// abort out of a helper whose job is exiting the whole position.
	const MIN_WITHDRAW_RAW = 1_000_000n;
	if (shares < MIN_WITHDRAW_RAW) {
		throw new Error(
			`${owner} holds ${shares} raw PLP shares; the minimum withdrawal request is ${MIN_WITHDRAW_RAW}.`,
		);
	}
	return client.predict.tx.withdrawPlp(owner, shares);
}

// A queued request can be cancelled up to the flush that would fill it. The
// index comes from the request receipt: `decode.plpRequest(result).index`.
export function cancelQueuedWithdraw(owner: string, index: bigint): Transaction {
	return client.predict.tx.cancelWithdrawPlp(owner, index);
}

// A queued supply cancels the same way, through its own builder.
export function cancelQueuedSupply(owner: string, index: bigint): Transaction {
	return client.predict.tx.cancelSupplyPlp(owner, index);
}
// docs::/#withdraw
