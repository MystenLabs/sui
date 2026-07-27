// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#read-risk
import type { DeepBookMarginClient } from './client.js';
import type { RiskParams } from './risk-params.js';

// Read your live risk ratio and see how close it is to liquidation.
// `getMarginManagerState` values your collateral and debt through the pool's
// Pyth oracles and returns the resulting risk ratio in one call, along with the
// oracle prices it used. Because it is oracle-priced, it moves whenever SUI
// moves, and it drifts down on its own as interest accrues on the debt.
export interface RiskStatus {
	riskRatio: number;
	baseDebt: string;
	quoteDebt: string;
	// Distance to the liquidation threshold, as a fraction of the current ratio.
	// Small and shrinking is the danger sign.
	marginToLiquidation: number;
	liquidatable: boolean;
}

export async function readRiskStatus(
	client: DeepBookMarginClient,
	marginManagerKey: string,
	params: RiskParams,
): Promise<RiskStatus> {
	const state = await client.deepbook.getMarginManagerState(marginManagerKey);
	const marginToLiquidation =
		(state.riskRatio - params.liquidationRiskRatio) / state.riskRatio;
	return {
		riskRatio: state.riskRatio,
		baseDebt: state.baseDebt,
		quoteDebt: state.quoteDebt,
		marginToLiquidation,
		liquidatable: state.riskRatio <= params.liquidationRiskRatio,
	};
}
// docs::/#read-risk
