// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { getObjectDisplay } from '@mysten/sui.js';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { useObjectsOwnedByAddress } from '_app/hooks/useObjectsOwnedByAddress';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import type {
    SuiAddress,
    PaginatedObjectsResponse,
    SuiObjectResponse,
    SuiObjectData,
    PaginationArguments,
} from '@mysten/sui.js';
//This is a temporary fix to get the NFT objects to show up in the wallet
// API Filters should be used to get the NFT objects

const MAX_FETCH_LIMIT = 5;
const DEFAULT_NFT_LIMIT = 5;

export function useGetNFTObjects(
    address?: SuiAddress | null,
    cursor?: PaginatedObjectsResponse['nextCursor']
) {
    const rpc = useRpcClient();
    // keep

    // return response;
    // next cursor
    return useInfiniteQuery(
        [
            '2get-object-nfts-q1121',
            '0xb027ec70fbfd1f661696bfbde4ce03ad052c774f17bfb0633ca577954bb04d89',
            cursor,
            'he',
        ],
        async ({ pageParam = cursor }) => {
            const nftsObjects: SuiObjectResponse[] = [];
            let hasNextPage = false;
            let currCursor = pageParam;
            // keep fetching until cursor is null or undefined
            do {
                const resp = await rpc.getOwnedObjects({
                    owner: '0xb027ec70fbfd1f661696bfbde4ce03ad052c774f17bfb0633ca577954bb04d89',
                    filter: { MatchNone: [{ StructType: '0x2::coin::Coin' }] },
                    options: {
                        showType: true,
                        showContent: true,
                        showDisplay: true,
                    },
                    cursor: currCursor,
                    limit: MAX_FETCH_LIMIT,
                });
                hasNextPage = resp?.hasNextPage || false;
                currCursor = resp?.nextCursor;
                console.log(resp.data.length, 'calling getOwnedObjects');
                if (!resp.data) {
                    break;
                }

                nftsObjects.push(
                    ...resp.data.filter((resp) => !!getObjectDisplay(resp).data)
                );
            } while (hasNextPage && nftsObjects.length < DEFAULT_NFT_LIMIT);
            console.log('nftsObjects1w', nftsObjects);
            return {
                data: nftsObjects,
                nextCursor: currCursor,
                hasNextPage: hasNextPage,
            };
        },
        {
            enabled: !!address,
            getNextPageParam: ({ nextCursor, hasNextPage }) =>
                hasNextPage ? nextCursor : null,
            // select: (data) =>
        }
    );
}
