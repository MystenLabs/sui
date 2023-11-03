// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSelectors } from '_app/zustand/createSelectors';
import { create } from 'zustand';

interface CoinsState {
	pinnedCoinTypes: string[];
	setPinnedCoinTypes: (pinnedCoins: string[]) => void;
}

const useCoinsStoreBase = create<CoinsState>()((set) => ({
	pinnedCoinTypes: [],
	setPinnedCoinTypes: (pinnedCoins: string[]) => set((state) => ({ pinnedCoinTypes: pinnedCoins })),
}));

export const useCoinsStore = createSelectors(useCoinsStoreBase);
