// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_EPOCHS_LIMIT = 20;

// Fetch paginated epochs
export function useGetEpochs(limit = DEFAULT_EPOCHS_LIMIT) {
    const rpc = useRpcClient();

    return useInfiniteQuery(
        ['get-epochs-blocks', limit],
        ({ pageParam = null }) =>
            rpc.getEpochs({
                descendingOrder: true,
                cursor: pageParam,
                limit,
            }),
        {
            getNextPageParam: ({ nextCursor, hasNextPage }) =>
                hasNextPage ? nextCursor : null,
            staleTime: 10 * 1000,
            cacheTime: 24 * 60 * 60 * 1000,
            retry: false,
            keepPreviousData: true,
        }
    );
}
