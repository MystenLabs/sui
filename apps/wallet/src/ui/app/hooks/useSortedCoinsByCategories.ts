// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { usePinnedCoinTypes } from '_app/hooks/usePinnedCoinTypes';
import { useRecognizedPackages } from '_app/hooks/useRecognizedPackages';
import { type CoinBalance as CoinBalanceType } from '@mysten/sui/client';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMemo } from 'react';

function sortCoins(balances: CoinBalanceType[]) {
	return balances.sort((a, b) => {
		if (a.coinType === SUI_TYPE_ARG) {
			return -1;
		}

		return a.coinType.toLowerCase().localeCompare(b.coinType.toLowerCase());
	});
}

export function useSortedCoinsByCategories(coinBalances: CoinBalanceType[]) {
	const recognizedPackages = useRecognizedPackages();
	const [pinnedCoinTypes] = usePinnedCoinTypes();

	return useMemo(() => {
		const reducedCoinBalances = coinBalances?.reduce(
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
		) ?? { recognized: [], pinned: [], unrecognized: [] };

		return {
			recognized: sortCoins(reducedCoinBalances.recognized),
			pinned: sortCoins(reducedCoinBalances.pinned),
			unrecognized: sortCoins(reducedCoinBalances.unrecognized),
		};
	}, [coinBalances, recognizedPackages, pinnedCoinTypes]);
}
