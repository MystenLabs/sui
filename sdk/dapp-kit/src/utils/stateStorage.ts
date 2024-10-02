// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { StateStorage } from 'zustand/middleware';

export function createInMemoryStore(): StateStorage {
	const store = new Map();
	return {
		getItem(key: string) {
			return store.get(key);
		},
		setItem(key: string, value: string) {
			store.set(key, value);
		},
		removeItem(key: string) {
			store.delete(key);
		},
	};
}
