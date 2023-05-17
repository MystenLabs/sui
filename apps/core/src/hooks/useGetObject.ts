// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { normalizeSuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const defaultOptions = {
    showType: true,
    showContent: true,
    showOwner: true,
    showPreviousTransaction: true,
    showStorageRebate: true,
    showDisplay: true,
};

export function useGetObject(objectId?: string | null) {
    const rpc = useRpcClient();
    const normalizedObjId = objectId && normalizeSuiAddress(objectId);
    return useQuery({
        queryKey: ['object', normalizedObjId],
        queryFn: () =>
            rpc.getObject({
                id: normalizedObjId!,
                options: defaultOptions,
            }),
        enabled: !!normalizedObjId,
    });
}

export function useGetObjects(objectIds?: string[]) {
    const rpc = useRpcClient();
    const normalizedObjIds =
        objectIds?.map((objectId) => normalizeSuiAddress(objectId)) || [];

    return useQuery({
        queryKey: ['objects', ...normalizedObjIds],
        queryFn: () => {
            const queries = normalizedObjIds.map((objectId) => {
                return rpc.getObject({
                    id: objectId,
                    options: defaultOptions,
                });
            });

            return Promise.all(queries);
        },
        enabled: !!normalizedObjIds,
    });
}
