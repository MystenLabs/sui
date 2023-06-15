// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface StorageAdapter {
	set(key: string, value: string): Promise<void>;
	get(key: string): Promise<string | undefined | null>;
	del(key: string): Promise<void>;
}

export const localStorageAdapter: StorageAdapter = {
	async set(key, value) {
		return localStorage.setItem(key, value);
	},
	async get(key) {
		return localStorage.getItem(key);
	},
	async del(key) {
		localStorage.removeItem(key);
	},
};

export const noopStorageAdapter: StorageAdapter = {
	async set() {},
	async get() {
		return null;
	},
	async del() {},
};
