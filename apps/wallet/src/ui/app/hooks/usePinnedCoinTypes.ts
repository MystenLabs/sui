// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { get, set } from 'idb-keyval';
import { useCallback, useEffect, useState } from 'react';

import { useRecognizedPackages } from './useRecognizedPackages';

const PINNED_COIN_TYPES = 'pinned-coin-types';

export function usePinnedCoinTypes() {
	const recognizedPackages = useRecognizedPackages();
	const [internalPinnedCoinTypes, internalSetPinnedCoinTypes] = useState<string[]>([]);

	// TODO: Ideally this should also update storage so that we don't need to keep track of pinned coins that have become recognized
	// In the event that a user pins a coin that becomes recognized, we need to remove it from pins:
	const pinnedCoinTypes = internalPinnedCoinTypes.filter(
		(coinType) => !recognizedPackages.includes(coinType.split('::')[0]),
	);

	useEffect(() => {
		(async () => {
			const pinnedCoins = await get<string[]>(PINNED_COIN_TYPES);
			if (pinnedCoins) {
				internalSetPinnedCoinTypes(pinnedCoins);
			}
		})();
	}, []);

	const pinCoinType = useCallback(
		async (newCoinType: string) => {
			if (pinnedCoinTypes.find((coinType) => coinType === newCoinType)) return;

			const newPinnedCoinTypes = [...pinnedCoinTypes, newCoinType];
			internalSetPinnedCoinTypes(newPinnedCoinTypes);
			await set(PINNED_COIN_TYPES, newPinnedCoinTypes);
		},
		[pinnedCoinTypes],
	);

	const unpinCoinType = useCallback(
		async (removeCoinType: string) => {
			const newPinnedCoinTypes = pinnedCoinTypes.filter((coinType) => coinType !== removeCoinType);
			internalSetPinnedCoinTypes(newPinnedCoinTypes);
			await set(PINNED_COIN_TYPES, newPinnedCoinTypes);
		},
		[pinnedCoinTypes],
	);

	return [pinnedCoinTypes, { pinCoinType, unpinCoinType }] as const;
}
