// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type express from 'express';

// Placeholder types for the agent sponsorship example
declare function verifyApiKey(apiKey: string): Promise<{ address: string; dailyGasBudget: number } | null>;
declare function getAgentDailySpend(address: string): Promise<number>;

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

	// ... same sponsorship flow as the /sponsor endpoint
}
// docs::/#agent-sponsor

export { handleAgentSponsor };
