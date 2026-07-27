// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#describe-pool
import type { DeepBookMarginClient } from './client.js';
import { readRiskParams, type RiskParams } from './risk-params.js';

// Preflight a DeepBook pool before integrating margin against it. A pool is
// tradeable on margin only if the registry has enabled it, and each side (base,
// quote) borrows from a separate isolated MarginPool. This reports whether the
// pool is enabled, the two margin pool IDs a position can borrow from, and the
// per-pool risk parameters, so you can gate your integration on real on-chain
// state instead of assuming a pool supports margin.
export interface MarginPoolProfile {
	poolKey: string;
	marginEnabled: boolean;
	baseMarginPoolId?: string;
	quoteMarginPoolId?: string;
	riskParams?: RiskParams;
}

export async function describeMarginPool(
	client: DeepBookMarginClient,
	poolKey: string,
): Promise<MarginPoolProfile> {
	const db = client.deepbook;
	const marginEnabled = await db.isPoolEnabledForMargin(poolKey);
	if (!marginEnabled) return { poolKey, marginEnabled };
	const [baseMarginPoolId, quoteMarginPoolId, riskParams] = await Promise.all([
		db.getBaseMarginPoolId(poolKey),
		db.getQuoteMarginPoolId(poolKey),
		readRiskParams(client, poolKey),
	]);
	return { poolKey, marginEnabled, baseMarginPoolId, quoteMarginPoolId, riskParams };
}
// docs::/#describe-pool
