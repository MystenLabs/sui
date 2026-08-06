// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type express from 'express';

import { sponsor } from './sponsor-sdk.js';

// Placeholder types for the agent sponsorship example
declare function verifyApiKey(
	apiKey: string,
): Promise<{ address: string; dailyGasBudget: number } | null>;
declare function getAgentDailySpend(address: string): Promise<number>;

// Upper bound for a single sponsored transaction. Keep in sync with the
// gasBudget({ max }) validator configured in sponsor-sdk.ts.
const MAX_GAS_BUDGET = 50_000_000;

// docs::#agent-sponsor
async function handleAgentSponsor(req: express.Request, res: express.Response) {
	const { txBytes, userSignature, apiKey } = req.body;

	// Verify agent identity
	const agent = await verifyApiKey(apiKey);
	if (!agent) {
		res.status(401).json({ error: 'Invalid API key' });
		return;
	}

	// Check per-agent daily budget before validating the transaction itself
	const todaySpend = await getAgentDailySpend(agent.address);
	if (todaySpend + MAX_GAS_BUDGET > agent.dailyGasBudget) {
		res.status(429).json({ error: 'Daily gas budget exceeded' });
		return;
	}

	// From here the flow is the standard sponsor flow. Validation rejections
	// are returned as part of the result, not thrown, so no error handling
	// beyond the discriminated union is needed.
	const result = await sponsor.signAndExecuteTransaction({
		transaction: txBytes,
		userSignature,
	});

	if (result.$kind === 'Rejected') {
		res.status(400).json({ error: 'Transaction rejected', issues: result.issues });
	} else if (result.$kind === 'FailedTransaction') {
		// Executed onchain but aborted; gas was charged and a digest exists,
		// so report it rather than retrying.
		res.status(422).json({
			digest: result.FailedTransaction.digest,
			status: result.FailedTransaction.effects.status,
		});
	} else {
		res.json({ digest: result.Transaction.digest });
	}
}
// docs::/#agent-sponsor

export { handleAgentSponsor };
