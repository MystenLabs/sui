// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { api } from '../redux/store/thunk-extras';

import type { SuiAddress, PaginatedCoins, CoinStruct } from '@mysten/sui.js';
// minimum number of coins to fetch per request
const MIN_COINS_PER_REQUEST = 100;

async function fetchCoins(address: SuiAddress, coinType: string) {
    const rpc = api.instance.fullNode;
    let cursor: string | null = null;
    const allData: CoinStruct[] = [];
    // keep fetching until cursor is null or undefined
    do {
        const { data, nextCursor } = (await rpc.getCoins(
            address,
            coinType,
            cursor,
            MIN_COINS_PER_REQUEST
        )) as PaginatedCoins;
        if (!data || !data.length) {
            break;
        }

        allData.push(...data);
        cursor = nextCursor;
    } while (cursor != null);

    return allData;
}

// Fetch all coins for an address, this will keep calling the API until all coins are fetched
export function useGetCoins(coinType: string, address?: SuiAddress | null) {
    return useQuery(
        ['get-coins', address, coinType],
        () => fetchCoins(address!, coinType),
        { enabled: !!address }
    );
}
