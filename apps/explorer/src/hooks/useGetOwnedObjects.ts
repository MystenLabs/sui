// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiObjectDataFilter, type SuiAddress } from '@mysten/sui.js';
import { useInfiniteQuery } from '@tanstack/react-query';

const MAX_OBJECTS_PER_REQ = 6;

export function useGetOwnedObjects(
    address?: SuiAddress | null,
    filter?: SuiObjectDataFilter
) {
    const rpc = useRpcClient();
    return useInfiniteQuery(
        ['get-owned-objects', address, filter],
        async ({ pageParam }) =>
            await rpc.getOwnedObjects({
                owner: address!,
                filter,
                options: {
                    showType: true,
                    showContent: true,
                    showDisplay: true,
                },
                limit: MAX_OBJECTS_PER_REQ,
                cursor: pageParam,
            }),
        {
            staleTime: 10 * 60 * 1000,
            enabled: !!address,
            getNextPageParam: (lastPage) =>
                lastPage?.hasNextPage ? lastPage.nextCursor : null,
        }
    );
}
