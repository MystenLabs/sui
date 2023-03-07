// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiObjectResponse, normalizeSuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

export function useGetSystemObject() {
    const rpc = useRpcClient();
    return useQuery(['system', 'state'], () => rpc.getSuiSystemState());
}

export function useGetObject(
    objectId: string
): UseQueryResult<SuiObjectResponse, unknown> {
    const rpc = useRpcClient();
    const normalizedObjId = normalizeSuiAddress(objectId);
    const response = useQuery(
        ['object', normalizedObjId],
        async () =>
            rpc.getObject(normalizedObjId, {
                showType: true,
                showContent: true,
                showOwner: true,
                showPreviousTransaction: true,
                showStorageRebate: true,
            }),
        { enabled: !!objectId }
    );

    return response;
}
