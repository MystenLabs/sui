// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * An sync key-value store.
 */
export interface SyncStore {
	get(key: string): string | null;
	set(key: string, value: string): void;
	delete(key: string): void;
}

function createWebStorage(storage: Storage): SyncStore {
	return {
		get(key: string) {
			return storage.getItem(key);
		},
		set(key: string, value: string) {
			storage.setItem(key, value);
		},
		delete(key: string) {
			storage.removeItem(key);
		},
	};
}

/**
 * Create a storage interface backed by memory.
 * This is generally useful for server-side rendering, and test environments.
 */
export function createInMemoryStorage(): SyncStore {
	const store = new Map<string, string>();
	return {
		get(key) {
			return store.get(key) ?? null;
		},
		set(key, value) {
			store.set(key, value);
		},
		delete(key) {
			store.delete(key);
		},
	};
}

/**
 * Create a store backed by `localStorage`.
 */
export function createLocalStorage(): SyncStore {
	if (typeof window === 'undefined') {
		console.warn('`window.localStorage` is not available, falling back to in-memory storage');
		return createInMemoryStorage();
	}

	return createWebStorage(window.localStorage);
}

/**
 * Create a store backed by `sessionStorage`.
 */
export function createSessionStorage(): SyncStore {
	if (typeof window === 'undefined') {
		console.warn('`window.sessionStorage` is not available, falling back to in-memory storage');
		return createInMemoryStorage();
	}

	return createWebStorage(window.sessionStorage);
}
