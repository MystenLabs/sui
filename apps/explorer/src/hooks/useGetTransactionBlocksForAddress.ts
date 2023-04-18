// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useInfiniteQuery } from '@tanstack/react-query';

import type { SuiAddress, TransactionFilter } from '@mysten/sui.js';

export const DEFAULT_TRANSACTIONS_LIMIT = 20;

// Fetch transaction blocks for an address, w/ toggle for to/from filter
export function useGetTransactionBlocksForAddress(
    address: SuiAddress,
    filter?: TransactionFilter,
    limit = DEFAULT_TRANSACTIONS_LIMIT
) {
    const rpc = useRpcClient();
    return useInfiniteQuery(
        ['get-transaction-blocks', address, filter, limit],
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
            enabled: !!address,
        }
    );
}
