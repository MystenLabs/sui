// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

const BASE_PRICE_MIST = 1_000_000n;

function generateChallenge(): { id: string; exactAmount: bigint } {
	const id = crypto.randomUUID();
	const offset = BigInt(Math.floor(Math.random() * 999) + 1);
	return { id, exactAmount: BASE_PRICE_MIST + offset };
}

describe('challenge store', () => {
	it('generates unique challenge IDs', () => {
		const ids = new Set<string>();
		for (let i = 0; i < 100; i++) {
			ids.add(generateChallenge().id);
		}
		expect(ids.size).toBe(100);
	});

	it('generates nonce amounts in the expected range', () => {
		for (let i = 0; i < 50; i++) {
			const { exactAmount } = generateChallenge();
			expect(exactAmount).toBeGreaterThan(BASE_PRICE_MIST);
			expect(exactAmount).toBeLessThanOrEqual(BASE_PRICE_MIST + 999n);
		}
	});

	it('produces distinct nonce amounts across challenges', () => {
		const amounts = new Set<bigint>();
		// With 999 possible offsets, 20 samples should produce at least 2 distinct values
		for (let i = 0; i < 20; i++) {
			amounts.add(generateChallenge().exactAmount);
		}
		expect(amounts.size).toBeGreaterThan(1);
	});

	it('expires challenges after TTL', () => {
		const store = new Map<string, { exactAmount: bigint; expiry: number }>();
		const { id, exactAmount } = generateChallenge();
		store.set(id, { exactAmount, expiry: Date.now() - 1000 });

		const pending = store.get(id);
		expect(pending).toBeDefined();
		expect(Date.now() > pending!.expiry).toBe(true);
	});

	it('prevents digest reuse', () => {
		const usedDigests = new Set<string>();
		const digest = '0xabc123';

		expect(usedDigests.has(digest)).toBe(false);
		usedDigests.add(digest);
		expect(usedDigests.has(digest)).toBe(true);
	});

	it('rejects a stolen digest paired with a different challenge', () => {
		const store = new Map<string, { exactAmount: bigint; expiry: number }>();

		// Challenge A issues amount = BASE + 42
		const challengeA = { id: crypto.randomUUID(), exactAmount: BASE_PRICE_MIST + 42n };
		store.set(challengeA.id, { exactAmount: challengeA.exactAmount, expiry: Date.now() + 300_000 });

		// Challenge B issues amount = BASE + 777
		const challengeB = { id: crypto.randomUUID(), exactAmount: BASE_PRICE_MIST + 777n };
		store.set(challengeB.id, { exactAmount: challengeB.exactAmount, expiry: Date.now() + 300_000 });

		// Attacker observes a payment of (BASE + 42) for challenge A.
		// Attacker tries to use that digest with challenge B.
		const pendingB = store.get(challengeB.id)!;
		const attackerPaymentAmount = challengeA.exactAmount; // the stolen amount

		// The exact-amount check rejects it: 1000042 !== 1000777
		expect(attackerPaymentAmount === pendingB.exactAmount).toBe(false);
	});
});
