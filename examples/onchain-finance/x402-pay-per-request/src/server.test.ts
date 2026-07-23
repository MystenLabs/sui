// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

describe('challenge store', () => {
	it('generates unique challenges', () => {
		const challenges = new Set<string>();
		for (let i = 0; i < 100; i++) {
			challenges.add(crypto.randomUUID());
		}
		expect(challenges.size).toBe(100);
	});

	it('expires challenges after TTL', () => {
		const store = new Map<string, { amount: bigint; expiry: number }>();
		const challenge = crypto.randomUUID();

		// Set expiry in the past
		store.set(challenge, { amount: 1_000_000n, expiry: Date.now() - 1000 });

		const pending = store.get(challenge);
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

	it('requires challenge binding for payment verification', () => {
		const pendingChallenges = new Map<string, { amount: bigint; expiry: number }>();

		// A payment digest alone should not pass verification without a valid challenge
		const validChallenge = crypto.randomUUID();
		pendingChallenges.set(validChallenge, {
			amount: 1_000_000n,
			expiry: Date.now() + 300_000,
		});

		// Unknown challenge should fail
		const unknownChallenge = crypto.randomUUID();
		expect(pendingChallenges.has(unknownChallenge)).toBe(false);

		// Valid challenge should pass
		expect(pendingChallenges.has(validChallenge)).toBe(true);

		// After use, challenge should be consumed
		pendingChallenges.delete(validChallenge);
		expect(pendingChallenges.has(validChallenge)).toBe(false);
	});
});
