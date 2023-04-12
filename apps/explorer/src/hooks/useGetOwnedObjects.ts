// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useInfiniteQuery } from '@tanstack/react-query';

const MAX_OBJECTS_PER_REQ = 6;

export function useGetOwnedObjects(address?: SuiAddress | null) {
    const rpc = useRpcClient();
    return useInfiniteQuery(
        ['get-owned-objects', address],
        async ({ pageParam }) =>
            await rpc.getOwnedObjects({
                owner: address!,
                options: {
                    showType: true,
                    showContent: true,
                    showDisplay: true,
                },
                limit: MAX_OBJECTS_PER_REQ,
                cursor: pageParam ? pageParam.cursor : null,
            }),
        {
            staleTime: 10 * 60 * 1000,
            enabled: !!address,
            getNextPageParam: (lastPage) =>
                lastPage?.hasNextPage
                    ? {
                          cursor: lastPage.nextCursor,
                      }
                    : false,
        }
    );
}
