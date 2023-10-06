// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filterAndSortTokenBalances } from '_helpers';
import { mainnetDeepBook, useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';

export const DEFAULT_MAX_SLIPPAGE_PERCENTAGE = '0.5';
export const FEES_PERCENTAGE = 0.03;

export const initialValues = {
	amount: '',
	isPayAll: false,
	quoteAssetType: mainnetDeepBook.coinsMap.USDC,
	allowedMaxSlippagePercentage: DEFAULT_MAX_SLIPPAGE_PERCENTAGE,
};

export type FormValues = typeof initialValues;

export function useCoinTypeData(activeCoinType: string | null) {
	const selectedAddress = useActiveAddress();

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coins, isLoading: coinsLoading } = useSuiClientQuery(
		'getAllBalances',
		{ owner: selectedAddress! },
		{
			enabled: !!selectedAddress,
			refetchInterval,
			staleTime,
			select: filterAndSortTokenBalances,
		},
	);

	const coin = coins?.find(({ coinType }) => coinType === activeCoinType);
	const coinBalance = coin?.totalBalance;
	const [formattedBalance] = useFormatCoin(coinBalance, activeCoinType);
	const coinMetadata = useCoinMetadata(activeCoinType);

	return {
		coin,
		formattedBalance,
		coinMetadata,
		isLoading: coinsLoading || coinMetadata.isLoading,
	};
}
