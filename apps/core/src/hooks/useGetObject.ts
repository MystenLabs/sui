// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { normalizeSuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetObject(objectId?: string | null) {
    const rpc = useRpcClient();
    const normalizedObjId = objectId && normalizeSuiAddress(objectId);
    return useQuery(
        ['object', normalizedObjId],
        async () =>
            rpc.getObject({
                id: normalizedObjId!,
                options: {
                    showType: true,
                    showContent: true,
                    showOwner: true,
                    showPreviousTransaction: true,
                    showStorageRebate: true,
                    showDisplay: true,
                },
            }),
        { enabled: !!normalizedObjId }
    );
}
