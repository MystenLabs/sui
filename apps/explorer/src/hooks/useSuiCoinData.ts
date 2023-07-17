// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

// TODO: We should consider using tRPC or something for apps-backend
type CoinData = {
	marketCap: string;
	fullyDilutedMarketCap: string;
	currentPrice: number;
	priceChangePercentageOver24H: number;
	circulatingSupply: number;
	totalSupply: number;
};

export function useSuiCoinData() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['sui-coin-data'],
		queryFn: () => request<CoinData>('coins/sui', {}),
		cacheTime: 24 * 60 * 60 * 1000,
		staleTime: Infinity,
	});
}
