// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiClient({ url: 'https://fullnode.mainnet.sui.io:443' });
const keypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

// docs::#fetch-with-payment
async function fetchWithPayment(url: string): Promise<Response> {
	// First attempt
	const response = await fetch(url);

	if (response.status !== 402) {
		return response;
	}

	// Parse payment instructions (includes server-issued challenge)
	const { amount, recipient, coinType, challenge } = await response.json();

	// Sign the challenge to prove we control the keypair.
	// The server will verify this signature and check that the onchain
	// transaction sender matches the recovered address.
	const challengeBytes = new TextEncoder().encode(challenge);
	const { signature: challengeSignature } = await keypair.signPersonalMessage(challengeBytes);

	// Build and submit payment
	const tx = new Transaction();
	tx.setSender(keypair.toSuiAddress());

	if (coinType === '0x2::sui::SUI') {
		const [coin] = tx.splitCoins(tx.gas, [BigInt(amount)]);
		tx.transferObjects([coin], recipient);
	} else {
		const { data: coins } = await client.getCoins({
			owner: keypair.toSuiAddress(),
			coinType,
		});
		const [coin] = tx.splitCoins(tx.object(coins[0].coinObjectId), [BigInt(amount)]);
		tx.transferObjects([coin], recipient);
	}

	const result = await client.signAndExecuteTransaction({
		transaction: tx,
		signer: keypair,
		options: { showEffects: true },
	});

	if (result.effects?.status.status !== 'success') {
		throw new Error('Payment transaction failed');
	}

	// Retry with payment proof, challenge ID, and signed challenge
	return fetch(url, {
		headers: {
			'X-Payment-Digest': result.digest,
			'X-Payment-Challenge': challenge,
			'X-Payment-Signature': challengeSignature,
		},
	});
}
// docs::/#fetch-with-payment

export { fetchWithPayment };
