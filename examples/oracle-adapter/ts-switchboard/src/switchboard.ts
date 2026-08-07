// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#switchboard
import { Transaction } from '@mysten/sui/transactions';
import type { SuiClient } from '@mysten/sui/client';
import { SwitchboardClient, Aggregator } from '@switchboard-xyz/sui-sdk';

// NOTE ON VERSIONS: this sample pins `@mysten/sui` 1.x, unlike the Pyth samples
// in `../ts`, which run on 2.x. `@switchboard-xyz/sui-sdk` has not migrated to
// the 2.x SDK, and the two `Transaction` types are not interchangeable, so a
// Switchboard consumer builds its transaction with the 1.x client today.
//
// Switchboard is on-demand: instead of a shared price object you update, you
// hold an Aggregator feed and pull a fresh oracle response into your
// transaction. `fetchUpdateTx` appends the update commands, and it MUST be the
// first action in the PTB so the feed is fresh before your consumer reads it.
//
// Your Move consumer then reads `switchboard::aggregator::current_result(agg)`,
// which returns a `CurrentResult` exposing `result()` (a `Decimal`) plus
// `min_timestamp_ms()` / `max_timestamp_ms()`. Unlike Pyth's
// `get_price_no_older_than`, `current_result` does not gate on age, so the
// consumer must check the timestamps against the clock itself.
export async function buildSwitchboardUpdateAndRead(
	sui: SuiClient,
	aggregatorId: string,
	consumerTarget: string,
): Promise<Transaction> {
	const sb = new SwitchboardClient(sui);
	const aggregator = new Aggregator(sb, aggregatorId);
	const tx = new Transaction();
	await aggregator.fetchUpdateTx(tx);
	tx.moveCall({
		target: consumerTarget,
		arguments: [tx.object(aggregatorId), tx.object.clock()],
	});
	return tx;
}
// docs::/#switchboard
