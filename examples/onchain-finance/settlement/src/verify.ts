// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient, SuiTransactionBlockResponse } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

declare const client: SuiClient;
declare const agentKeypair: Ed25519Keypair;
declare const tx: Transaction;

// docs::#wait-for-finality
// Submit the transaction
const submitResult = await client.signAndExecuteTransaction({
	transaction: tx,
	signer: agentKeypair,
	options: { showEffects: true },
});

// Wait for the transaction to be indexed
await client.waitForTransaction({ digest: submitResult.digest });

// Fetch full effects for verification
const confirmed = await client.getTransactionBlock({
	digest: submitResult.digest,
	options: { showBalanceChanges: true, showEvents: true, showEffects: true },
});
// docs::/#wait-for-finality

// docs::#assert-success
function assertSuccess(result: SuiTransactionBlockResponse) {
	if (result.effects?.status.status !== 'success') {
		throw new Error(
			`Transaction failed: ${result.effects?.status.error}`,
		);
	}
}
// docs::/#assert-success

// docs::#verify-balance-changes
function verifyPayment(
	result: SuiTransactionBlockResponse,
	expectedRecipient: string,
	expectedAmount: bigint,
	expectedCoinType: string,
): boolean {
	const changes = result.balanceChanges ?? [];

	// Find the recipient's positive balance change
	const recipientChange = changes.find((c) => {
		const owner = c.owner;
		return (
			typeof owner === 'object' &&
			'AddressOwner' in owner &&
			owner.AddressOwner === expectedRecipient &&
			c.coinType === expectedCoinType &&
			BigInt(c.amount) > 0n
		);
	});

	if (!recipientChange) {
		return false; // No matching change found
	}

	return BigInt(recipientChange.amount) >= expectedAmount;
}

// Usage
const isValid = verifyPayment(
	confirmed,
	recipientAddress,
	5_000_000n, // 5 USDC
	'0xdba...::usdc::USDC',
);

if (!isValid) {
	throw new Error('Payment verification failed');
}
// docs::/#verify-balance-changes

declare const recipientAddress: string;

// docs::#verify-payment-kit-events
function verifyPaymentKitEvent(
	result: SuiTransactionBlockResponse,
	expectedNonce: string,
	expectedAmount: bigint,
	expectedRecipient: string,
): boolean {
	const events = result.events ?? [];

	const paymentEvent = events.find(
		(e) =>
			e.type.includes('payment_kit::PaymentProcessed') &&
			(e.parsedJson as any)?.nonce === expectedNonce,
	);

	if (!paymentEvent) {
		return false;
	}

	const data = paymentEvent.parsedJson as any;
	return (
		BigInt(data.payment_amount) >= expectedAmount &&
		data.receiver === expectedRecipient
	);
}
// docs::/#verify-payment-kit-events

// docs::#validate-address
// Validate before building
if (recipientAddress.length !== 66 || !recipientAddress.startsWith('0x')) {
	throw new Error('Invalid recipient address');
}
// docs::/#validate-address

// docs::#handle-timeout
try {
	await client.waitForTransaction({ digest, timeout: 30_000 });
} catch (timeoutError) {
	// Check if the transaction eventually settled
	try {
		const timeoutResult = await client.getTransactionBlock({
			digest,
			options: { showEffects: true },
		});
		// Transaction settled; verify effects
	} catch {
		// Transaction never settled; safe to retry with same idempotency key
		throw new Error('Transaction not found; retry is safe');
	}
}
// docs::/#handle-timeout

declare const digest: string;

// docs::#onchain-settlement-ptb
const PACKAGE_ID = '0xPACKAGE';
const recipient = '0xRECIPIENT';
const amount = 5_000_000n;

const settleTx = new Transaction();

const [coin] = settleTx.splitCoins(settleTx.gas, [amount]);

// Pay and get proof
const [proof] = settleTx.moveCall({
	target: `${PACKAGE_ID}::settlement::pay_and_prove`,
	typeArguments: ['0x2::sui::SUI'],
	arguments: [coin, settleTx.pure.address(recipient)],
});

// Verify the proof (mandatory, hot potato)
settleTx.moveCall({
	target: `${PACKAGE_ID}::settlement::verify_settlement`,
	arguments: [
		proof,
		settleTx.pure.address(recipient),
		settleTx.pure.u64(amount),
	],
});
// docs::/#onchain-settlement-ptb

export { assertSuccess, verifyPayment, verifyPaymentKitEvent };
