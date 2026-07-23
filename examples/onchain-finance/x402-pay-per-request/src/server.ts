// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from 'node:crypto';
import express from 'express';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { verifyPersonalMessageSignature } from '@mysten/sui/verify';

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
// Each challenge is a random token the client must sign with their keypair.
// The signature proves the requester controls the private key that sent
// the onchain payment. Without this, an attacker who observes a public
// digest could declare the real payer's address and steal the resource.
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
	const challengeSignature = req.headers['x-payment-signature'] as string;

	if (!digest || !challengeId || !challengeSignature) {
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
				'Payment required. Sign the challenge with your keypair, pay the amount, then retry with X-Payment-Digest, X-Payment-Challenge, and X-Payment-Signature headers.',
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
	const challengeSignature = req.headers['x-payment-signature'] as string;

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
		// 3. Verify the challenge signature to recover the signer's address.
		//    This proves the requester controls the private key.
		const challengeBytes = new TextEncoder().encode(challengeId);
		const signerAddress = await verifyPersonalMessageSignature(
			challengeBytes,
			challengeSignature,
		);

		// 4. Fetch the transaction and verify the sender matches the signer
		const result = await client.core.getTransaction({
			digest,
			include: { balanceChanges: true, transaction: true },
		});

		if (result.$kind === 'FailedTransaction') {
			res.status(402).json({ error: 'Transaction failed' });
			return;
		}

		const tx = result.Transaction!;

		if (tx.transaction?.sender !== signerAddress) {
			res.status(403).json({
				error: 'Transaction sender does not match challenge signer',
			});
			return;
		}

		// 5. Verify the server received the expected amount
		const balanceChanges = tx.balanceChanges ?? [];
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

		// 6. Consume challenge and mark digest as used
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
