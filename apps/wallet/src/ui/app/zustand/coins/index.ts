// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { set as idbSet } from 'idb-keyval';
import { create } from 'zustand';

interface CoinsState {
	pinnedCoinTypes: string[];
	setPinnedCoinTypes: (key: string, pinnedCoins: string[]) => void;
}

const coinsStoreBase = create<CoinsState>();

export const useCoinsStore = coinsStoreBase((set) => ({
	pinnedCoinTypes: [],
	setPinnedCoinTypes: async (key, pinnedCoins) => {
		await idbSet(key, pinnedCoins);
		set(() => ({ pinnedCoinTypes: pinnedCoins }));
	},
}));
