// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from './useAppsBackend';

type CoinData = {
    marketCap: number;
    fullyDilutedMarketCap: number;
    currentPrice: number;
    priceChangePercentageOver24H: number;
};

export function useSuiCoinData() {
    const makeAppsBackendRequest = useAppsBackend();
    return useQuery(
        ['sui-coin-data'],
        () => makeAppsBackendRequest<CoinData>('coins/aptos', {}),
        {
            // Cache this forever because we have limited API bandwidth at the moment
            cacheTime: Infinity,
        }
    );
}
