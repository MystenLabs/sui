// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_EPOCHS_LIMIT = 20;

// Fetch transaction blocks
export function useGetCheckpoints(limit = DEFAULT_EPOCHS_LIMIT) {
    const rpc = useRpcClient();

    return useInfiniteQuery(
        ['get-checkpoints', limit],
        async ({ pageParam }) =>
            await rpc.getCheckpoints({
                descendingOrder: true,
                cursor: pageParam,
                limit,
            }),
        {
            getNextPageParam: (lastPage) =>
                lastPage?.hasNextPage ? lastPage.nextCursor : false,
            staleTime: Infinity,
            cacheTime: 24 * 60 * 60 * 1000,
            retry: false,
            keepPreviousData: true,
        }
    );
}
