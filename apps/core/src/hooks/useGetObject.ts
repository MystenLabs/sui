// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { SuiObjectResponse, normalizeSuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const defaultOptions = {
    showType: true,
    showContent: true,
    showOwner: true,
    showPreviousTransaction: true,
    showStorageRebate: true,
    showDisplay: true,
};

export function useGetObject(
    objectId?: string | null,
    version?: string | null
) {
    const rpc = useRpcClient();
    const normalizedObjId = objectId && normalizeSuiAddress(objectId);
    return useQuery<SuiObjectResponse>({
        queryKey: ['object', normalizedObjId, version],
        queryFn: async () => {
            if (version) {
                const data = await rpc.tryGetPastObject({
                    id: normalizedObjId!,
                    options: defaultOptions,
                    version: Number(version),
                });

                if (data.status !== 'VersionFound') {
                    throw new Error(data.status);
                }

                return { data: data.details };
            }

            return rpc.getObject({
                id: normalizedObjId!,
                options: defaultOptions,
            });
        },
        enabled: !!normalizedObjId,
    });
}
