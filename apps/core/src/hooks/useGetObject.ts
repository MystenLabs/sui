// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { normalizeSuiAddress, type SuiObjectDataOptions } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const defaultOptions: SuiObjectDataOptions = {
    showType: true,
    showContent: true,
    showOwner: true,
    showDisplay: true,
};

export function useGetObject(
    objectId?: string | null,
    options = defaultOptions
) {
    const rpc = useRpcClient();
    const normalizedObjId = objectId && normalizeSuiAddress(objectId);
    return useQuery(
        ['object', normalizedObjId],
        async () =>
            rpc.getObject({
                id: normalizedObjId!,
                options,
            }),
        { enabled: !!normalizedObjId }
    );
}
