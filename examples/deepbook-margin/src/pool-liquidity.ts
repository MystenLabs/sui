// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#pool-liquidity
import type { DeepBookMarginClient } from './client.js';

// You borrow from a margin pool, so it must hold enough idle supply to lend.
// Borrowing is capped by the pool's max utilization rate: the most that can be
// borrowed is `maxUtilizationRate * totalSupply`, and anything already borrowed
// counts against it. A pool with no headroom fails a borrow the same way a thin
// order book fails a trade, so check before you borrow. The interest rate rises
// with utilization, so a nearly full pool is also an expensive one.
export interface BorrowLiquidity {
	totalSupply: number;
	totalBorrow: number;
	maxUtilizationRate: number;
	interestRate: number; // current borrow APR, moves with utilization
	borrowableNow: number; // headroom before the utilization cap
}

export async function readBorrowLiquidity(
	client: DeepBookMarginClient,
	coinKey: string,
): Promise<BorrowLiquidity> {
	const db = client.deepbook;
	const [supplyStr, borrowStr, maxUtilizationRate, interestRate] = await Promise.all([
		db.getMarginPoolTotalSupply(coinKey),
		db.getMarginPoolTotalBorrow(coinKey),
		db.getMarginPoolMaxUtilizationRate(coinKey),
		db.getMarginPoolInterestRate(coinKey),
	]);
	// Supply and borrow come back as decimal strings; the rates as numbers.
	const totalSupply = Number(supplyStr);
	const totalBorrow = Number(borrowStr);
	const borrowableNow = Math.max(0, maxUtilizationRate * totalSupply - totalBorrow);
	return { totalSupply, totalBorrow, maxUtilizationRate, interestRate, borrowableNow };
}
// docs::/#pool-liquidity
