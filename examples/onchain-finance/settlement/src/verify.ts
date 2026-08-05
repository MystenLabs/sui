// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ClientWithCoreApi, SuiClientTypes } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { isValidSuiAddress, normalizeStructTag, normalizeSuiAddress } from '@mysten/sui/utils';

declare const client: ClientWithCoreApi;
declare const agentKeypair: Ed25519Keypair;
declare const tx: Transaction;
declare const recipientAddress: string;
declare const digest: string;

// docs::#wait-for-finality
// Submit the transaction
const submitResult = await client.signAndExecuteTransaction({
    transaction: tx,
    signer: agentKeypair,
    include: { effects: true },
});

const submitted = submitResult.Transaction ?? submitResult.FailedTransaction;

// Wait for the transaction to be indexed, then fetch the data needed for verification
const confirmed = await client.waitForTransaction({
    digest: submitted.digest,
    include: { effects: true, events: true, balanceChanges: true },
});
// docs::/#wait-for-finality

// docs::#assert-success
// Execution results are a discriminated union: a transaction can be final onchain
// and still have aborted, so always check the variant before trusting the effects.
function assertSuccess(result: SuiClientTypes.TransactionResult) {
    if (result.$kind === 'FailedTransaction') {
        throw new Error(`Transaction failed: ${result.FailedTransaction.status.error}`);
    }

    return result.Transaction;
}
// docs::/#assert-success

// docs::#verify-balance-changes
function verifyPayment(
    result: SuiClientTypes.TransactionResult,
    expectedRecipient: string,
    expectedAmount: bigint,
    expectedCoinType: string,
): boolean {
    const transaction = assertSuccess(result);
    const changes = transaction.balanceChanges ?? [];

    const expectedAddress = normalizeSuiAddress(expectedRecipient);
    const expectedType = normalizeStructTag(expectedCoinType);

    // Find the recipient's positive balance change
    const recipientChange = changes.find(
        (change) =>
            normalizeSuiAddress(change.address) === expectedAddress &&
            normalizeStructTag(change.coinType) === expectedType &&
            BigInt(change.amount) > 0n,
    );

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
    '0xUSDC_PACKAGE::usdc::USDC',
);

if (!isValid) {
    throw new Error('Payment verification failed');
}
// docs::/#verify-balance-changes

// docs::#verify-payment-kit-events
interface PaymentReceiptEvent {
    nonce: string;
    payment_amount: string;
    receiver: string;
    coin_type: string;
}

function isPaymentReceiptEvent(value: unknown): value is PaymentReceiptEvent {
    if (typeof value !== 'object' || value === null) {
        return false;
    }

    const event = value as Record<string, unknown>;
    return (
        typeof event.nonce === 'string' &&
        typeof event.payment_amount === 'string' &&
        typeof event.receiver === 'string' &&
        typeof event.coin_type === 'string'
    );
}

function verifyPaymentKitEvent(
    result: SuiClientTypes.TransactionResult,
    auditedPackageId: string,
    expectedNonce: string,
    expectedAmount: bigint,
    expectedRecipient: string,
    expectedCoinType: string,
): boolean {
    const transaction = assertSuccess(result);

    const expectedEventType = normalizeStructTag(`${auditedPackageId}::payment_kit::PaymentReceipt`);
    const expectedReceiver = normalizeSuiAddress(expectedRecipient);
    const normalizedCoinType = normalizeStructTag(expectedCoinType);

    return (transaction.events ?? []).some((event) => {
        if (
            normalizeStructTag(event.eventType) !== expectedEventType ||
            !isPaymentReceiptEvent(event.json)
        ) {
            return false;
        }

        return (
            event.json.nonce === expectedNonce &&
            BigInt(event.json.payment_amount) === expectedAmount &&
            normalizeSuiAddress(event.json.receiver) === expectedReceiver &&
            normalizeStructTag(event.json.coin_type) === normalizedCoinType
        );
    });
}
// docs::/#verify-payment-kit-events

// docs::#validate-address
// Validate before building
if (!isValidSuiAddress(recipientAddress)) {
    throw new Error('Invalid recipient address');
}
// docs::/#validate-address

// docs::#handle-timeout
try {
    await client.waitForTransaction({ digest, timeout: 30_000 });
} catch (timeoutError) {
    // Check whether the transaction eventually settled
    try {
        const settled = await client.getTransaction({
            digest,
            include: { effects: true },
        });

        // It settled after all, so verify the effects rather than retrying
        assertSuccess(settled);
    } catch {
        // Never settled. Transactions built with a client expire at the end of the
        // next epoch, so retrying under the same idempotency key is safe.
        throw new Error('Transaction not found; retry is safe');
    }
}
// docs::/#handle-timeout

// docs::#onchain-settlement-ptb
const PACKAGE_ID = '0xPACKAGE';
const recipient = '0xRECIPIENT';
const amount = 5_000_000n;

const settleTx = new Transaction();

// Pay and get proof (hot potato)
const [proof] = settleTx.moveCall({
    target: `${PACKAGE_ID}::settlement::pay_and_prove`,
    typeArguments: ['0x2::sui::SUI'],
    arguments: [settleTx.coin({ balance: amount }), settleTx.pure.address(recipient), settleTx.pure.u64(amount)],
});

// Consume the proof (mandatory, hot potato)
settleTx.moveCall({
    target: `${PACKAGE_ID}::settlement::consume_proof`,
    arguments: [proof],
});
// docs::/#onchain-settlement-ptb

export { assertSuccess, verifyPayment, verifyPaymentKitEvent };
