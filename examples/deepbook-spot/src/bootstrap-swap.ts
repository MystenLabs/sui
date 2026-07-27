// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#bootstrap-swap
import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookTestnetClient } from './client.js';

// Swap SUI for DEEP on the DEEP_SUI Testnet pool. That pool is whitelisted
// (zero fee) and the swap needs no BalanceManager, so it bootstraps DEEP from
// faucet SUI. DEEP is the base and SUI the quote, so a quote-for-base swap
// spends SUI and returns DEEP. Set `minDeepOut` to a nonzero value (for example
// 99% of getQuantityOut's `baseOut`): with `minOut: 0`, a thin or empty book
// silently returns your SUI unfilled instead of reverting.
export function swapSuiForDeep(
	client: DeepBookTestnetClient,
	suiAmount: number,
	minDeepOut: number,
	recipient: string,
): Transaction {
	const tx = new Transaction();
	const [deepOut, suiRemainder, deepFee] = tx.add(
		client.deepbook.deepBook.swapExactQuoteForBase({
			poolKey: 'DEEP_SUI',
			amount: suiAmount,
			deepAmount: 0,
			minOut: minDeepOut,
		}),
	);
	tx.transferObjects([deepOut, suiRemainder, deepFee], recipient);
	return tx;
}
// docs::/#bootstrap-swap
