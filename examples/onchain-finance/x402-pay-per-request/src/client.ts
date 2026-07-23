// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const keypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

// docs::#fetch-with-payment
async function fetchWithPayment(url: string): Promise<Response> {
	const senderAddress = keypair.toSuiAddress();

	// First attempt — include sender address so the server can bind the challenge
	const response = await fetch(url, {
		headers: { 'X-Payment-Sender': senderAddress },
	});

	if (response.status !== 402) {
		return response;
	}

	// Parse payment instructions (includes server-issued challenge bound to our address)
	const { amount, recipient, coinType, challenge } = await response.json();

	// Build and submit payment
	const tx = new Transaction();
	tx.setSender(keypair.toSuiAddress());

	if (coinType === '0x2::sui::SUI') {
		const [coin] = tx.splitCoins(tx.gas, [BigInt(amount)]);
		tx.transferObjects([coin], recipient);
	} else {
		// For non-SUI coins, select a coin object of the right type
		const coins = await client.listCoins({
			owner: keypair.toSuiAddress(),
			coinType,
		});
		const [coin] = tx.splitCoins(tx.object(coins.objects[0].objectId), [BigInt(amount)]);
		tx.transferObjects([coin], recipient);
	}

	const result = await client.signAndExecuteTransaction({
		transaction: tx,
		signer: keypair,
	});

	if (result.$kind === 'FailedTransaction') {
		throw new Error('Payment transaction failed');
	}

	const digest = result.Transaction!.digest;

	// Retry with payment proof and the server-issued challenge
	return fetch(url, {
		headers: {
			'X-Payment-Digest': digest,
			'X-Payment-Challenge': challenge,
		},
	});
}
// docs::/#fetch-with-payment

export { fetchWithPayment };
