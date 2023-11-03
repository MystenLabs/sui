// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSelectors } from '_app/zustand/createSelectors';
import { set as idbSet } from 'idb-keyval';
import { create } from 'zustand';

interface CoinsState {
	pinnedCoinTypes: string[];
	setPinnedCoinTypes: (key: string, pinnedCoins: string[]) => void;
}

const coinsStoreBase = create<CoinsState>();

const useCoinsStoreBase = coinsStoreBase((set) => ({
	pinnedCoinTypes: [],
	setPinnedCoinTypes: async (key, pinnedCoins) => {
		await idbSet(key, pinnedCoins);
		set(() => ({ pinnedCoinTypes: pinnedCoins }));
	},
}));

export const useCoinsStore = createSelectors(useCoinsStoreBase);
