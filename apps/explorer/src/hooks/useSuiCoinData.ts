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

export function useSuiCoinData() {
    const makeAppsBackendRequest = useAppsBackend();
    return useQuery(
        ['sui-coin-data'],
        () => makeAppsBackendRequest<CoinData>('coins/sui', {}),
        {
            cacheTime: 24 * 60 * 60 * 1000,
            staleTime: Infinity,
        }
    );
}
