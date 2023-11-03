// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinsStore } from '_app/zustand/coins';
import { get, set } from 'idb-keyval';
import { useCallback, useEffect } from 'react';

import { useRecognizedPackages } from './useRecognizedPackages';

const PINNED_COIN_TYPES = 'pinned-coin-types';

export function usePinnedCoinTypes() {
	const setPinnedCoinTypes = useCoinsStore.use.setPinnedCoinTypes();
	const internalPinnedCoinTypes = useCoinsStore.use.pinnedCoinTypes();
	const recognizedPackages = useRecognizedPackages();

	// TODO: Ideally this should also update storage so that we don't need to keep track of pinned coins that have become recognized
	// In the event that a user pins a coin that becomes recognized, we need to remove it from pins:
	const pinnedCoinTypes = internalPinnedCoinTypes.filter(
		(coinType) => !recognizedPackages.includes(coinType.split('::')[0]),
	);

	useEffect(() => {
		(async () => {
			const pinnedCoins = await get<string[]>(PINNED_COIN_TYPES);
			if (pinnedCoins) {
				setPinnedCoinTypes(pinnedCoins);
			}
		})();
	}, [setPinnedCoinTypes]);

	const pinCoinType = useCallback(
		async (newCoinType: string) => {
			if (pinnedCoinTypes.find((coinType) => coinType === newCoinType)) return;

			const newPinnedCoinTypes = [...pinnedCoinTypes, newCoinType];
			setPinnedCoinTypes(newPinnedCoinTypes);
			await set(PINNED_COIN_TYPES, newPinnedCoinTypes);
		},
		[pinnedCoinTypes, setPinnedCoinTypes],
	);

	const unpinCoinType = useCallback(
		async (removeCoinType: string) => {
			const newPinnedCoinTypes = pinnedCoinTypes.filter((coinType) => coinType !== removeCoinType);
			setPinnedCoinTypes(newPinnedCoinTypes);
			await set(PINNED_COIN_TYPES, newPinnedCoinTypes);
		},
		[pinnedCoinTypes, setPinnedCoinTypes],
	);

	return [pinnedCoinTypes, { pinCoinType, unpinCoinType }] as const;
}
