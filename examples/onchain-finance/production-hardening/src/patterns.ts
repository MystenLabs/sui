// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

declare const client: SuiClient;
declare const signer: Ed25519Keypair;

// docs::#kill-switch
function assertNotPaused() {
	if (process.env.AGENT_PAUSED === 'true') {
		throw new Error('Agent is paused via kill-switch');
	}
}

// Check before every transaction
assertNotPaused();
const tx = new Transaction();
const result = await client.signAndExecuteTransaction({ transaction: tx, signer });
// docs::/#kill-switch

// docs::#safe-execute
interface IdempotencyStore {
	get(key: string): Promise<any | null>;
	set(key: string, value: any): Promise<void>;
}

async function safeExecute(
	suiClient: SuiClient,
	transaction: Transaction,
	keypair: Ed25519Keypair,
	idempotencyKey: string,
	db: IdempotencyStore,
) {
	// Check if this operation already succeeded
	const existing = await db.get(idempotencyKey);
	if (existing) {
		return existing;
	}

	const execResult = await suiClient.signAndExecuteTransaction({
		transaction,
		signer: keypair,
		options: { showEffects: true },
	});

	if (execResult.effects?.status.status !== 'success') {
		throw new Error(`Transaction failed: ${execResult.effects?.status.error}`);
	}

	// Record success
	await db.set(idempotencyKey, execResult);

	// Wait for indexing
	await suiClient.waitForTransaction({ digest: execResult.digest });

	return execResult;
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
		const breakerResult = await client.signAndExecuteTransaction({ transaction, signer });
		breaker.recordSuccess();
		return breakerResult;
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
declare const agentAddress: string;
declare const idempotencyKey: string;
declare const amount: bigint;
declare const recipient: string;

console.log(JSON.stringify({
	event: 'tx_attempt',
	agent: agentAddress,
	idempotencyKey,
	digest: result.digest,
	status: result.effects?.status.status,
	amount: amount.toString(),
	recipient,
	timestamp: new Date().toISOString(),
}));
// docs::/#structured-log

export { CircuitBreaker, RateLimiter, safeExecute, executeWithBreaker };
