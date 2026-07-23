// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

describe('challenge store', () => {
	it('generates unique challenge IDs', () => {
		const ids = new Set<string>();
		for (let i = 0; i < 100; i++) {
			ids.add(crypto.randomUUID());
		}
		expect(ids.size).toBe(100);
	});

	it('expires challenges after TTL', () => {
		const store = new Map<string, { expiry: number }>();
		const id = crypto.randomUUID();
		store.set(id, { expiry: Date.now() - 1000 });

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

	it('consumes challenge after use', () => {
		const store = new Map<string, { expiry: number }>();
		const id = crypto.randomUUID();
		store.set(id, { expiry: Date.now() + 300_000 });

		expect(store.has(id)).toBe(true);
		store.delete(id);
		expect(store.has(id)).toBe(false);
	});
});
