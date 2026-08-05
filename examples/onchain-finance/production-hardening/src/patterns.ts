// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ClientWithCoreApi } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

declare const client: ClientWithCoreApi;
declare const signer: Ed25519Keypair;
declare const recipient: string;
declare const amount: bigint;
declare const idempotencyKey: string;

// docs::#kill-switch
function assertNotPaused() {
    if (process.env.AGENT_PAUSED === 'true') {
        throw new Error('Agent is paused via kill-switch');
    }
}

// Check before every transaction
assertNotPaused();

const tx = new Transaction();
tx.transferObjects([tx.coin({ balance: amount })], recipient);

const result = await client.signAndExecuteTransaction({
    transaction: tx,
    signer,
    include: { effects: true },
});

if (result.$kind === 'FailedTransaction') {
    throw new Error(`Transaction failed: ${result.FailedTransaction.status.error}`);
}
// docs::/#kill-switch

// docs::#safe-execute
type IdempotencyRecord =
    | { status: 'submitting'; digest: string }
    | { status: 'succeeded'; digest: string };

interface IdempotencyStore {
    get(key: string): Promise<IdempotencyRecord | null>;
    claim(key: string, record: IdempotencyRecord): Promise<boolean>;
    markSucceeded(key: string, digest: string): Promise<void>;
}

// A FailedTransaction IS onchain (sender was charged gas, tx has effects).
// This function returns the successful tx, throws on abort, or returns null if never seen.
async function reconcileDigest(coreClient: ClientWithCoreApi, digest: string) {
    try {
        const onchain = await coreClient.core.getTransaction({
            digest,
            include: { effects: true },
        });

        if (onchain.$kind === 'FailedTransaction') {
            throw new Error(`Transaction failed: ${onchain.FailedTransaction.status.error}`);
        }

        return onchain.Transaction;
    } catch (error) {
        if (error instanceof Error && error.message.includes('not found')) {
            return null;
        }
        throw error;
    }
}

async function safeExecute(
    coreClient: ClientWithCoreApi,
    transaction: Transaction,
    keypair: Ed25519Keypair,
    key: string,
    db: IdempotencyStore,
) {
    const existing = await db.get(key);

    if (existing) {
        const onchain = await reconcileDigest(coreClient, existing.digest);

        if (onchain) {
            await db.markSucceeded(key, existing.digest);
            return onchain;
        }

        if (existing.status === 'succeeded') {
            throw new Error('Recorded transaction was not found onchain');
        }

        // Transactions built with a client expire at the end of the next epoch, so an
        // unresolved submission that never landed can be rebuilt and retried safely.
        throw new Error('Submission is unresolved; reconcile it before retrying');
    }

    // Pin the sender so the digest computed below matches the bytes that get signed.
    transaction.setSender(keypair.toSuiAddress());

    const bytes = await transaction.build({ client: coreClient });
    const { signature } = await keypair.signTransaction(bytes);
    const digest = await transaction.getDigest({ client: coreClient });

    const claimed = await db.claim(key, { status: 'submitting', digest });
    if (!claimed) {
        throw new Error('Another worker is processing this idempotency key');
    }

    try {
        const execResult = await coreClient.core.executeTransaction({ // .core is correct here: coreClient is ClientWithCoreApi
            transaction: bytes,
            signatures: [signature],
            include: { effects: true },
        });

        if (execResult.$kind === 'FailedTransaction') {
            throw new Error(`Transaction failed: ${execResult.FailedTransaction.status.error}`);
        }

        if (execResult.Transaction.digest !== digest) {
            throw new Error('Node returned an unexpected transaction digest');
        }

        await coreClient.core.waitForTransaction({ digest });
        await db.markSucceeded(key, digest);

        return execResult.Transaction;
    } catch (error) {
        const onchain = await reconcileDigest(coreClient, digest);

        if (onchain) {
            await db.markSucceeded(key, digest);
            return onchain;
        }

        throw error;
    }
}
// docs::/#safe-execute

// docs::#circuit-breaker
class CircuitBreaker {
    private failures = 0;
    private lastFailure = 0;

    constructor(
        private maxFailures: number = 5,
        private resetAfterMs: number = 60_000,
    ) {}

    recordSuccess() {
        this.failures = 0;
    }

    recordFailure() {
        this.failures++;
        this.lastFailure = Date.now();
    }

    isOpen(): boolean {
        // Reset if enough time has passed since the last failure
        if (this.failures > 0 && Date.now() - this.lastFailure > this.resetAfterMs) {
            this.failures = 0;
            return false;
        }
        return this.failures >= this.maxFailures;
    }
}

// Usage
const breaker = new CircuitBreaker(5, 60_000);

async function executeWithBreaker(transaction: Transaction) {
    if (breaker.isOpen()) {
        throw new Error('Circuit breaker open: too many consecutive failures');
    }

    try {
        const breakerResult = await client.signAndExecuteTransaction({
            transaction,
            signer,
            include: { effects: true },
        });

        if (breakerResult.$kind === 'FailedTransaction') {
            breaker.recordFailure();
            throw new Error(`Transaction failed: ${breakerResult.FailedTransaction.status.error}`);
        }

        breaker.recordSuccess();
        return breakerResult.Transaction;
    } catch (error) {
        breaker.recordFailure();
        throw error;
    }
}
// docs::/#circuit-breaker

// docs::#rate-limiter
class RateLimiter {
    private timestamps: number[] = [];

    constructor(
        private maxRequests: number,
        private windowMs: number,
    ) {}

    tryAcquire(): boolean {
        const now = Date.now();
        this.timestamps = this.timestamps.filter((t) => now - t < this.windowMs);

        if (this.timestamps.length >= this.maxRequests) {
            return false;
        }

        this.timestamps.push(now);
        return true;
    }
}

// Allow at most 10 transactions per minute
const limiter = new RateLimiter(10, 60_000);

if (!limiter.tryAcquire()) {
    throw new Error('Rate limit exceeded');
}
// docs::/#rate-limiter

// docs::#structured-log
const logged = result.Transaction ?? result.FailedTransaction;

console.log(
    JSON.stringify({
        event: 'tx_attempt',
        agent: signer.toSuiAddress(),
        idempotencyKey,
        digest: logged.digest,
        success: logged.status.success,
        error: logged.status.error ?? null,
        amount: amount.toString(),
        recipient,
        timestamp: new Date().toISOString(),
    }),
);
// docs::/#structured-log

export { assertNotPaused, CircuitBreaker, executeWithBreaker, RateLimiter, safeExecute };
