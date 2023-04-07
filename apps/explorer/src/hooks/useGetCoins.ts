// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import type { SuiAddress } from '@mysten/sui.js';

const MAX_COINS_PER_REQUEST = 10;

// Fetch all coins for an address, this will keep calling the API until all coins are fetched
export function useGetCoins(
    coinType: string,
    address?: SuiAddress | null,
    cursor?: string | null,
    cacheTime = 0
) {
    const rpc = useRpcClient();
    return useQuery(
        ['get-coins', address, coinType],
        async () =>
            await rpc.getCoins({
                owner: address!,
                coinType,
                cursor,
                limit: MAX_COINS_PER_REQUEST,
            }),
        {
            enabled: !!address,
            cacheTime,
        }
    );
}
