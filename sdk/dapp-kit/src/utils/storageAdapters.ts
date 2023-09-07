// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: This is currently only used for storing wallet information, but we should
// figure out how this functionality ties into supporting react-query persistence
// when we have more of the base infrastructure set up.
export interface StorageAdapter {
	set(key: string, value: string): Promise<void>;
	remove(key: string): Promise<void>;
	get(key: string): Promise<string | undefined | null>;
}

export const localStorageAdapter: StorageAdapter = {
	async set(key, value) {
		return localStorage.setItem(key, value);
	},
	async remove(key) {
		localStorage.removeItem(key);
	},
	async get(key) {
		return localStorage.getItem(key);
	},
};

export const noopStorageAdapter: StorageAdapter = {
	async set() {},
	async remove() {},
	async get() {
		return null;
	},
};
