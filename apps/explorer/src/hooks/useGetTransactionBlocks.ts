// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useInfiniteQuery } from '@tanstack/react-query';

import type { TransactionFilter } from '@mysten/sui.js';

export const DEFAULT_TRANSACTIONS_LIMIT = 20;

// Fetch transaction blocks
export function useGetTransactionBlocks(
    filter?: TransactionFilter,
    limit = DEFAULT_TRANSACTIONS_LIMIT
) {
    const rpc = useRpcClient();

    return useInfiniteQuery(
        ['get-transaction-blocks', filter, limit],
        async ({ pageParam }) =>
            await rpc.queryTransactionBlocks({
                filter,
                cursor: pageParam,
                order: 'descending',
                limit,
                options: {
                    showEffects: true,
                    showBalanceChanges: true,
                    showInput: true,
                },
            }),
        {
            getNextPageParam: (lastPage) =>
                lastPage?.hasNextPage ? lastPage.nextCursor : false,
            staleTime: 10 * 1000,
            retry: false,
            keepPreviousData: true,
        }
    );
}
