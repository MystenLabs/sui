// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiObjectResponse, normalizeSuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

export function useGetObject(
    objectId: string
): UseQueryResult<SuiObjectResponse, unknown> {
    const rpc = useRpcClient();
    const normalizedObjId = normalizeSuiAddress(objectId);
    const response = useQuery(
        ['object', normalizedObjId],
        async () => {
            return rpc.getObject({
                id: normalizedObjId,
                options: {
                    showType: true,
                    showContent: true,
                    showOwner: true,
                },
            });
        },
        { enabled: !!objectId }
    );

    return response;
}
