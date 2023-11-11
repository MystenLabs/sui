// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getCookies } from 'next-client-cookies/server';
import type { StateStorage } from 'zustand/middleware';

import { readStorageState } from '../../walletStore.js';

// A minimal cookie interface that's compatible with next-client-cookies::
export interface NextClientCookies {
	set(name: string, value: string, options?: unknown): void;
	get(name: string): string | undefined;
	remove(name: string, options?: unknown): void;
}

export function createCookieStore(
	cookies: NextClientCookies,
	onValueChange?: () => void,
): StateStorage {
	return {
		getItem(key: string) {
			return cookies.get(key) ?? null;
		},
		setItem(key: string, value: string) {
			const previousValue = cookies.get(key);
			cookies.set(key, value);

			// Allow consumers to be notified of a value change:
			if (previousValue !== value) {
				onValueChange?.();
			}
		},
		removeItem(key: string) {
			cookies.remove(key);
		},
	};
}

// TODO: Maybe move these to `/server` imports?
export async function getConnectedAddress(storageKey?: string): Promise<string | null> {
	const state = await readStorageState(createCookieStore(getCookies()), storageKey);

	if (!state) return null;

	return state.lastConnectedAccountAddress;
}

export async function getConnectedWallet(storageKey?: string): Promise<string | null> {
	const state = await readStorageState(createCookieStore(getCookies()), storageKey);

	if (!state) return null;

	return state.lastConnectedWalletName;
}
