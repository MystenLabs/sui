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
// Replay prevention uses two mechanisms:
// 1. Each challenge ID is single-use — deleted after successful verification.
// 2. Each transaction digest is tracked globally — a digest accepted once
//    cannot be reused for any future challenge.
interface PendingChallenge {
	expiry: number;
}

const pendingChallenges = new Map<string, PendingChallenge>();
const usedDigests = new Set<string>();

function generateChallengeId(): string {
	return crypto.randomUUID();
}
// docs::/#challenge-store

// docs::#payment-required
const paymentRequired: express.RequestHandler = (req, res, next) => {
	const digest = req.headers['x-payment-digest'] as string;
	const challengeId = req.headers['x-payment-challenge'] as string;

	if (!digest || !challengeId) {
		const id = generateChallengeId();
		pendingChallenges.set(id, {
			expiry: Date.now() + 5 * 60 * 1000, // 5 minute window
		});

		res.status(402).json({
			amount: PRICE_MIST.toString(),
			recipient: PAYMENT_RECIPIENT,
			coinType: COIN_TYPE,
			challenge: id,
			message:
				'Payment required. Pay the amount, then retry with X-Payment-Digest and X-Payment-Challenge headers.',
		});
		return;
	}

	next();
};
// docs::/#payment-required

// docs::#verify-payment
const verifyPayment: express.RequestHandler = async (req, res, next) => {
	const digest = req.headers['x-payment-digest'] as string;
	const challengeId = req.headers['x-payment-challenge'] as string;

	// 1. Verify the challenge was issued by this server and hasn't expired
	const pending = pendingChallenges.get(challengeId);
	if (!pending || Date.now() > pending.expiry) {
		res.status(400).json({ error: 'Invalid or expired challenge' });
		return;
	}

	// 2. Prevent digest reuse across all challenges
	if (usedDigests.has(digest)) {
		res.status(400).json({ error: 'Payment digest already used' });
		return;
	}

	try {
		const result = await client.getTransaction({
			digest,
			include: { balanceChanges: true },
		});

		if (result.$kind === 'FailedTransaction') {
			res.status(402).json({ error: 'Transaction failed' });
			return;
		}

		const tx = result.Transaction!;
		const balanceChanges = tx.balanceChanges ?? [];

		// 3. Verify the server received the expected amount
		const received = balanceChanges.find(
			(change) =>
				change.address === PAYMENT_RECIPIENT &&
				change.coinType === COIN_TYPE &&
				BigInt(change.amount) >= PRICE_MIST,
		);

		if (!received) {
			res.status(402).json({ error: 'Payment not found or insufficient amount' });
			return;
		}

		// 4. Consume challenge and mark digest as used
		usedDigests.add(digest);
		pendingChallenges.delete(challengeId);
		next();
	} catch {
		res.status(402).json({ error: 'Could not verify payment' });
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
