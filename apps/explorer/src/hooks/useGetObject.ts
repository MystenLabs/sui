// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetObjectDataResponse,
    normalizeSuiAddress,
} from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from './useRpc';

export function useGetValidators() {
    const rpc = useRpc();
    return useQuery(['system', 'validators'], () => rpc.getValidators());
}

export function useGetSystemObject() {
    const rpc = useRpc();
    return useQuery(['system', 'state'], () => rpc.getSuiSystemState());
}

export function useGetObject(
    objectId: string
): UseQueryResult<GetObjectDataResponse, unknown> {
    const rpc = useRpc();
    const normalizedObjId = normalizeSuiAddress(objectId);
    const response = useQuery(
        ['object', normalizedObjId],
        async () => rpc.getObject(normalizedObjId),
        { enabled: !!objectId }
    );

    return response;
}
