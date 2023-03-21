// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import {
    type SuiObjectResponse,
    normalizeSuiAddress,
    type SuiObjectDataOptions,
} from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

export function useGetObject(
    objectId: string,
    options?: SuiObjectDataOptions
): UseQueryResult<SuiObjectResponse, unknown> {
    const adjOptions = {
        showType: true,
        showContent: true,
        showOwner: true,
        ...options,
    };
    const rpc = useRpcClient();
    const normalizedObjId = normalizeSuiAddress(objectId);
    const response = useQuery(
        ['object', normalizedObjId, adjOptions],
        async () => {
            return rpc.getObject({
                id: normalizedObjId,
                options: adjOptions,
            });
        },
        { enabled: !!objectId }
    );

    return response;
}
