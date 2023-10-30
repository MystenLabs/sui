// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from './useAppsBackend';

// TODO: We should consider using tRPC or something for apps-backend
type CoinData = {
	marketCap: string;
	fullyDilutedMarketCap: string;
	currentPrice: number;
	priceChangePercentageOver24H: number;
	circulatingSupply: number;
	totalSupply: number;
};

export const COIN_GECKO_SUI_URL = 'https://www.coingecko.com/en/coins/sui';

export function useSuiCoinData() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['sui-coin-data'],
		queryFn: () => request<CoinData>('coins/sui', {}),
		gcTime: 24 * 60 * 60 * 1000,
		staleTime: Infinity,
	});
}
