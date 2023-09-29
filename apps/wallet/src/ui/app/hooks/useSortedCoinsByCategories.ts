// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { usePinnedCoinTypes } from '_app/hooks/usePinnedCoinTypes';
import { useRecognizedPackages } from '_app/hooks/useRecognizedPackages';
import { type CoinBalance as CoinBalanceType } from '@mysten/sui.js/client';
import { useMemo } from 'react';

export function useSortedCoinsByCategories(coinBalances: CoinBalanceType[]) {
	const recognizedPackages = useRecognizedPackages();
	const [pinnedCoinTypes] = usePinnedCoinTypes();

	return useMemo(
		() =>
			coinBalances?.reduce(
				(acc, coinBalance) => {
					if (recognizedPackages.includes(coinBalance.coinType.split('::')[0])) {
						acc.recognized.push(coinBalance);
					} else if (pinnedCoinTypes.includes(coinBalance.coinType)) {
						acc.pinned.push(coinBalance);
					} else {
						acc.unrecognized.push(coinBalance);
					}
					return acc;
				},
				{
					recognized: [] as CoinBalanceType[],
					pinned: [] as CoinBalanceType[],
					unrecognized: [] as CoinBalanceType[],
				},
			) ?? { recognized: [], pinned: [], unrecognized: [] },
		[coinBalances, recognizedPackages, pinnedCoinTypes],
	);
}
