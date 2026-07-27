// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#read-fees
import type { DeepBookTestnetClient } from './client.js';

// Taker fee, maker fee, and stake requirement are per-pool governance
// parameters. Read them on-chain instead of hardcoding, because governance can
// change them. Fees are returned in billionths (divide by 1e9 for a fraction).
export async function readPoolFees(client: DeepBookTestnetClient, poolKey: string) {
	const params = await client.deepbook.poolTradeParams(poolKey);
	return params; // { takerFee, makerFee, stakeRequired }
}
// docs::/#read-fees
