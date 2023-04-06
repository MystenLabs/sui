// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import type { SuiAddress, PaginatedCoins, CoinStruct } from '@mysten/sui.js';
import { useRpcClient } from '../api/RpcClientContext';

// Fetch all coins for an address
export function useGetCoins(coinType: string,
    address?: SuiAddress | null,
    cursor?: string | null,
    MAX_COINS_PER_REQUEST = 10
) {
    const rpc = useRpcClient();
    return useQuery(
        ['get-coins', address, coinType],
        async () => {
            // const coinData: CoinStruct[] = []
            const data: PaginatedCoins = await rpc.getCoins(
                {
                    owner: address!,
                    coinType,
                    cursor,
                    limit: MAX_COINS_PER_REQUEST,
                }
            );
            // let cursor: string | null = null;
            // const allData: CoinStruct[] = [];
            // // keep fetching until cursor is null or undefined
            // do {
            //     const { data, nextCursor }: PaginatedCoins = await rpc.getCoins(
            //         {
            //             owner: address!,
            //             coinType,
            //             cursor,
            //             limit: MAX_COINS_PER_REQUEST,
            //         }
            //     );
            //     if (!data || !data.length) {
            //         break;
            //     }

            //     allData.push(...data);
            //     cursor = nextCursor;
            // } while (cursor);

            // return allData;

            return data
        },
        { enabled: !!address }
    );
}
