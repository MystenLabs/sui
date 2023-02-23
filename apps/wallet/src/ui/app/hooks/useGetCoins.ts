// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { api } from '../redux/store/thunk-extras';

// minimum number of coins to fetch per request
const MIN_COINS_PER_REQUEST = 100;

async function fetchCoins(address: SuiAddress, coinType: string) {
    const rpc = api.instance.fullNode;
    let cursor: string | null = null;
    const firstPageCoins = await rpc.getCoins(
        address,
        coinType,
        cursor,
        MIN_COINS_PER_REQUEST
    );
    if (!firstPageCoins) {
        return [];
    }

    const allData = [...firstPageCoins.data];
    cursor = firstPageCoins?.nextCursor || null;

    // keep fetching until cursor is null or undefined
    while (cursor) {
        const data = await rpc.getCoins(
            address,
            coinType,
            cursor,
            MIN_COINS_PER_REQUEST
        );
        if (!data) {
            // if data is null, then we should stop fetching
            cursor = null;
        }

        allData.push(...data.data);
        cursor = data.nextCursor;
    }

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
