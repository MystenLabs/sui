// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#risk-params
import type { DeepBookMarginClient } from './client.js';

// A pool's risk thresholds and liquidation rewards are stored per pool in the
// MarginRegistry and are set by governance, not fixed in code. Read them on
// chain rather than hardcoding: the values below differ from the protocol
// defaults and can change. `liquidation < minBorrow < minWithdraw` always holds.
export interface RiskParams {
	liquidationRiskRatio: number; // at or below this, the position can be liquidated
	minBorrowRiskRatio: number; // a borrow must leave the ratio at or above this
	minWithdrawRiskRatio: number; // a withdraw must leave the ratio at or above this
	targetLiquidationRiskRatio: number; // liquidation restores the ratio to this
	userLiquidationReward: number; // fraction of collateral paid to the liquidator
	poolLiquidationReward: number; // fraction of collateral paid to the pool
}

export async function readRiskParams(
	client: DeepBookMarginClient,
	poolKey: string,
): Promise<RiskParams> {
	const db = client.deepbook;
	const [
		liquidationRiskRatio,
		minBorrowRiskRatio,
		minWithdrawRiskRatio,
		targetLiquidationRiskRatio,
		userLiquidationReward,
		poolLiquidationReward,
	] = await Promise.all([
		db.getLiquidationRiskRatio(poolKey),
		db.getMinBorrowRiskRatio(poolKey),
		db.getMinWithdrawRiskRatio(poolKey),
		db.getTargetLiquidationRiskRatio(poolKey),
		db.getUserLiquidationReward(poolKey),
		db.getPoolLiquidationReward(poolKey),
	]);
	return {
		liquidationRiskRatio,
		minBorrowRiskRatio,
		minWithdrawRiskRatio,
		targetLiquidationRiskRatio,
		userLiquidationReward,
		poolLiquidationReward,
	};
}
// docs::/#risk-params
