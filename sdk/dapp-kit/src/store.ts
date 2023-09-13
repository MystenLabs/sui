// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { createStore } from 'zustand';
import type { StateStorage } from 'zustand/middleware';
import { persist, createJSONStorage, subscribeWithSelector } from 'zustand/middleware';
import type { WalletSlice } from './slices/walletSlice.js';
import { createWalletSlice } from './slices/walletSlice.js';

export type DAppKitStore = ReturnType<typeof createDAppKitStore>;

export type StoreState = WalletSlice;

export type DAppKitConfiguration = {
	wallets: WalletWithRequiredFeatures[];
	storage: StateStorage;
	storageKey: string;
};

export function createDAppKitStore({ wallets, storage, storageKey }: DAppKitConfiguration) {
	return createStore<StoreState>()(
		subscribeWithSelector(
			persist(createWalletSlice(wallets), {
				name: storageKey,
				storage: createJSONStorage(() => storage),
				partialize: ({ lastWalletName, lastAccountAddress }) => ({
					lastWalletName,
					lastAccountAddress,
				}),
			}),
		),
	);
}
