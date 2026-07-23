// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from 'node:crypto';
import express from 'express';
import { SuiGrpcClient } from '@mysten/sui/grpc';

// docs::#config
const PAYMENT_RECIPIENT = '0xYOUR_SERVER_ADDRESS';
const BASE_PRICE_MIST = 1_000_000n; // 0.001 SUI
const COIN_TYPE = '0x2::sui::SUI';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
// docs::/#config

// docs::#challenge-store
// Each challenge binds a payment to a specific request via a unique nonce amount.
// The server picks BASE_PRICE + random offset; the client must pay that exact amount.
// This prevents an attacker from stealing an observed digest — the amount won't match
// a different challenge's nonce.
interface PendingChallenge {
	exactAmount: bigint;
	expiry: number;
}

const pendingChallenges = new Map<string, PendingChallenge>();
const usedDigests = new Set<string>();

function generateChallenge(): { id: string; exactAmount: bigint } {
	const id = crypto.randomUUID();
	// Add a random offset of 1–999 MIST to the base price.
	// This makes each challenge's expected amount unique, binding
	// the onchain payment to this specific challenge.
	const offset = BigInt(Math.floor(Math.random() * 999) + 1);
	return { id, exactAmount: BASE_PRICE_MIST + offset };
}
// docs::/#challenge-store

// docs::#payment-required
const paymentRequired: express.RequestHandler = (req, res, next) => {
	const digest = req.headers['x-payment-digest'] as string;
	const challengeId = req.headers['x-payment-challenge'] as string;

	if (!digest || !challengeId) {
		const { id, exactAmount } = generateChallenge();
		pendingChallenges.set(id, {
			exactAmount,
			expiry: Date.now() + 5 * 60 * 1000, // 5 minute window
		});

		res.status(402).json({
			amount: exactAmount.toString(),
			recipient: PAYMENT_RECIPIENT,
			coinType: COIN_TYPE,
			challenge: id,
			message:
				'Payment required. Pay the exact amount, then retry with X-Payment-Digest and X-Payment-Challenge headers.',
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

	// Verify the challenge was issued by this server and hasn't expired
	const pending = pendingChallenges.get(challengeId);
	if (!pending || Date.now() > pending.expiry) {
		res.status(400).json({ error: 'Invalid or expired challenge' });
		return;
	}

	// Prevent digest reuse
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

		// Verify the server received the exact nonce amount.
		// The exact amount ties this payment to this specific challenge.
		const received = balanceChanges.find(
			(change) =>
				change.address === PAYMENT_RECIPIENT &&
				change.coinType === COIN_TYPE &&
				BigInt(change.amount) === pending.exactAmount,
		);

		if (!received) {
			res.status(402).json({ error: 'Payment not found or amount does not match challenge' });
			return;
		}

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
