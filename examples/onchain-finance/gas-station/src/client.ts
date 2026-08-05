// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { fromBase64, normalizeSuiAddress, toBase64 } from '@mysten/sui/utils';
import { z } from 'zod';

const client = new SuiGrpcClient({
	network: 'testnet',
	baseUrl: 'https://fullnode.testnet.sui.io:443',
});
const user = Ed25519Keypair.fromSecretKey(process.env.USER_SECRET_KEY!);
const gasStationUrl = 'http://localhost:3001';
const apiKey = process.env.SPONSOR_API_KEY!;
const expectedSponsor = normalizeSuiAddress(process.env.SPONSOR_ADDRESS!);
const maxGasBudget = 10_000_000n;

const SponsoredResponseSchema = z.object({
	txBytes: z.string(),
	sponsorSignature: z.string(),
	sponsorAddress: z.string(),
	gasCoinId: z.string(),
	digest: z.string(),
});

// Node's Buffer is not available in a browser, and this flow usually runs in
// one. Compare the bytes directly instead.
function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
	if (a.length !== b.length) return false;
	let difference = 0;
	for (let i = 0; i < a.length; i++) difference |= a[i] ^ b[i];
	return difference === 0;
}

// docs::#client-flow
const tx = new Transaction();
tx.moveCall({ target: '0xPACKAGE::module::function' });

const kindBytes = await tx.build({ client, onlyTransactionKind: true });
const response = await fetch(`${gasStationUrl}/sponsor`, {
	method: 'POST',
	headers: {
		Authorization: `Bearer ${apiKey}`,
		'Content-Type': 'application/json',
	},
	body: JSON.stringify({ txBytes: toBase64(kindBytes), sender: user.toSuiAddress() }),
});
if (!response.ok) throw new Error(`Sponsorship rejected: ${response.status}`);

const sponsored = SponsoredResponseSchema.parse(await response.json());

// Verify everything the gas station added before crossing the signing boundary.
const finalBytes = fromBase64(sponsored.txBytes);
const finalTx = Transaction.from(finalBytes);
const finalData = finalTx.getData();
const returnedKind = await finalTx.build({ client, onlyTransactionKind: true });
if (!bytesEqual(returnedKind, kindBytes)) {
	throw new Error('Gas station changed the transaction kind');
}
if (normalizeSuiAddress(finalData.sender!) !== user.toSuiAddress()) {
	throw new Error('Gas station changed the sender');
}
if (
	normalizeSuiAddress(finalData.gasConfig.owner!) !== expectedSponsor ||
	normalizeSuiAddress(sponsored.sponsorAddress) !== expectedSponsor ||
	finalData.gasConfig.payment?.length !== 1 ||
	BigInt(finalData.gasConfig.budget ?? 0) > maxGasBudget ||
	finalData.gasConfig.payment[0].objectId !== sponsored.gasCoinId
) {
	throw new Error('Gas station returned gas data outside policy');
}

const simulation = await client.simulateTransaction({ transaction: finalBytes });
if (!simulation.transaction.effects.status.success) {
	throw new Error('Sponsored transaction simulation failed');
}
const userSig = await user.signTransaction(finalBytes);

try {
	await client.executeTransaction({
		transaction: finalBytes,
		signatures: [userSig.signature, sponsored.sponsorSignature],
	});
} finally {
	// Confirm the digest the client inspected, even if execution reports an
	// error: failed transactions can still consume gas and advance the coin.
	await fetch(`${gasStationUrl}/sponsor/confirm`, {
		method: 'POST',
		headers: {
			Authorization: `Bearer ${apiKey}`,
			'Content-Type': 'application/json',
		},
		body: JSON.stringify({ gasCoinId: sponsored.gasCoinId, digest: sponsored.digest }),
	});
}
// docs::/#client-flow
