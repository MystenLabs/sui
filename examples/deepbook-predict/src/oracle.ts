// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#oracle
import { PREDICT, type ActiveOracle } from './config.js';

type ServerOracle = {
	oracle_id: string;
	expiry: number;
	min_strike: number;
	tick_size: number;
	status: string; // "inactive" | "active" | "pending_settlement" | "settled"
};

// Picks the first active oracle and a strike `tickIndex` ticks up the grid.
export async function getActiveOracle(
	predictObjectId: string,
	tickIndex = 0,
): Promise<ActiveOracle> {
	const res = await fetch(`${PREDICT.serverUrl}/predicts/${predictObjectId}/oracles`);
	if (!res.ok) throw new Error(`oracle fetch failed: ${res.status}`);
	const oracles = (await res.json()) as ServerOracle[];
	const live = oracles.find((o) => o.status === 'active');
	if (!live) throw new Error('no active oracle available');
	return {
		oracleId: live.oracle_id,
		expiry: live.expiry,
		strike: live.min_strike + tickIndex * live.tick_size,
	};
}
// docs::/#oracle
