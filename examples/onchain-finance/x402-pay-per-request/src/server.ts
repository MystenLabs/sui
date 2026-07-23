// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from 'node:crypto';
import express from 'express';
import { SuiGrpcClient } from '@mysten/sui/grpc';

// docs::#config
const PAYMENT_RECIPIENT = '0xYOUR_SERVER_ADDRESS';
const PRICE_MIST = 1_000_000n; // 0.001 SUI
const COIN_TYPE = '0x2::sui::SUI';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
// docs::/#config

// docs::#challenge-store
// Track used challenges to prevent replay.
// Each challenge binds a payment to a specific request.
const pendingChallenges = new Map<string, { amount: bigint; expiry: number }>();
const usedDigests = new Set<string>();

function generateChallenge(): string {
	return crypto.randomUUID();
}
// docs::/#challenge-store

// docs::#payment-required
const paymentRequired = (
	req: express.Request,
	res: express.Response,
	next: express.NextFunction,
) => {
	const digest = req.headers['x-payment-digest'] as string;
	const challenge = req.headers['x-payment-challenge'] as string;

	if (!digest || !challenge) {
		// Issue a unique challenge that the client must return with payment proof
		const newChallenge = generateChallenge();
		pendingChallenges.set(newChallenge, {
			amount: PRICE_MIST,
			expiry: Date.now() + 5 * 60 * 1000, // 5 minute window
		});

		return res.status(402).json({
			amount: PRICE_MIST.toString(),
			recipient: PAYMENT_RECIPIENT,
			coinType: COIN_TYPE,
			challenge: newChallenge,
			message:
				'Payment required. Submit a Sui transaction, then retry with X-Payment-Digest and X-Payment-Challenge headers.',
		});
	}

	next();
};
// docs::/#payment-required

// docs::#verify-payment
const verifyPayment = async (
	req: express.Request,
	res: express.Response,
	next: express.NextFunction,
) => {
	const digest = req.headers['x-payment-digest'] as string;
	const challenge = req.headers['x-payment-challenge'] as string;

	// Verify the challenge was issued by this server and hasn't expired
	const pending = pendingChallenges.get(challenge);
	if (!pending || Date.now() > pending.expiry) {
		return res.status(400).json({ error: 'Invalid or expired challenge' });
	}

	// Prevent digest reuse
	if (usedDigests.has(digest)) {
		return res.status(400).json({ error: 'Payment digest already used' });
	}

	try {
		const result = await client.getTransaction({
			digest,
			include: { balanceChanges: true },
		});

		if (result.$kind === 'FailedTransaction') {
			return res.status(402).json({ error: 'Transaction failed' });
		}

		const tx = result.Transaction!;
		const balanceChanges = tx.balanceChanges ?? [];

		// Verify the server received the expected amount.
		// gRPC balance changes use `address`, not `owner`.
		const received = balanceChanges.find(
			(change) =>
				change.address === PAYMENT_RECIPIENT &&
				change.coinType === COIN_TYPE &&
				BigInt(change.amount) >= pending.amount,
		);

		if (!received) {
			return res.status(402).json({ error: 'Payment not found or insufficient amount' });
		}

		usedDigests.add(digest);
		pendingChallenges.delete(challenge);
		next();
	} catch {
		return res.status(402).json({ error: 'Could not verify payment' });
	}
};
// docs::/#verify-payment

// docs::#app
const app = express();

app.get('/api/resource', paymentRequired, verifyPayment, (_req, res) => {
	res.json({ data: 'Protected resource content' });
});

app.listen(3000);
// docs::/#app
