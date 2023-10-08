// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * An asynchronous key-value store.
 */
export interface AsyncStore {
	get(key: string): Promise<string | null>;
	set(key: string, value: string): Promise<void>;
	delete(key: string): Promise<void>;
}

function createWebStorage(storage: Storage): AsyncStore {
	return {
		async get(key: string) {
			return storage.getItem(key);
		},
		async set(key: string, value: string) {
			storage.setItem(key, value);
		},
		async delete(key: string) {
			storage.removeItem(key);
		},
	};
}

/**
 * Create a story backed by memory.
 * This is generally useful for server-side rendering, and test environments.
 */
export function createInMemoryStorage(): AsyncStore {
	const store = new Map<string, string>();
	return {
		async get(key) {
			return store.get(key) ?? null;
		},
		async set(key, value) {
			store.set(key, value);
		},
		async delete(key) {
			store.delete(key);
		},
	};
}

/**
 * Create a store backed by `localStorage`.
 */
export function createLocalStorage(): AsyncStore {
	if (typeof window === 'undefined') {
		console.warn('`window.localStorage` is not available, falling back to in-memory storage');
		return createInMemoryStorage();
	}

	return createWebStorage(window.localStorage);
}

/**
 * Create a store backed by `sessionStorage`.
 */
export function createSessionStorage(): AsyncStore {
	if (typeof window === 'undefined') {
		console.warn('`window.sessionStorage` is not available, falling back to in-memory storage');
		return createInMemoryStorage();
	}

	return createWebStorage(window.sessionStorage);
}
