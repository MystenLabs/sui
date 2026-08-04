// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type express from 'express';

// Placeholder types for the agent sponsorship example
declare function verifyApiKey(apiKey: string): Promise<{ address: string; dailyGasBudget: number } | null>;
declare function getAgentDailySpend(address: string): Promise<number>;
declare function sponsorTransaction(txBytes: string, sender: string): Promise<{
    txBytes: string;
    sponsorSignature: string;
    sponsorAddress: string;
    gasCoinId: string;
    digest: string;
}>;

// Keep in sync with GAS_BUDGET in server.ts.
const GAS_BUDGET = 10_000_000;

// docs::#agent-sponsor
async function handleAgentSponsor(req: express.Request, res: express.Response) {
	const { txBytes, apiKey } = req.body;

	// Verify agent identity
	const agent = await verifyApiKey(apiKey);
	if (!agent) {
		res.status(401).json({ error: 'Invalid API key' });
		return;
	}

	// Check per-agent daily budget
	const todaySpend = await getAgentDailySpend(agent.address);
	if (todaySpend + GAS_BUDGET > agent.dailyGasBudget) {
		res.status(429).json({ error: 'Daily gas budget exceeded' });
		return;
	}

	// From here the flow is identical to /sponsor: validate the transaction
	// kind against the allowlist, acquire a gas coin, set gas data, simulate,
	// then sign.
	try {
		res.json(await sponsorTransaction(txBytes, agent.address));
	} catch (error) {
		res.status(400).json({ error: (error as Error).message });
	}
}
// docs::/#agent-sponsor

export { handleAgentSponsor };
