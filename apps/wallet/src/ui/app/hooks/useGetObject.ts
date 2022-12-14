// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type GetObjectDataResponse } from '@mysten/sui.js';
import {
    useQuery,
    type UseQueryResult,
    useQueryClient,
} from '@tanstack/react-query';

import { useRpc } from '_hooks';

export function useGetObject(
    objectId: string
): UseQueryResult<GetObjectDataResponse, unknown> {
    const rpc = useRpc();
    const response = useQuery(
        ['object', objectId],
        async () => {
            return rpc.getObject(objectId);
        },
        { enabled: !!objectId }
    );

    return response;
}

// Invalidates the cache Objects. called after a transaction
export function useValidateObjectQueryCache(cacheId: string) {
    const queryClient = useQueryClient();
    queryClient.invalidateQueries({ queryKey: [cacheId] });
}
