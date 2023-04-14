// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

type DynamicFieldName = {
    type: string;
    value?: string;
};

export function useGetDynamicFieldObject(
    parentId: string,
    name: DynamicFieldName
) {
    const rpc = useRpcClient();
    return useQuery(
        ['dynamic-fields-object', parentId, name],
        () =>
            rpc.getDynamicFieldObject({
                parentId,
                name,
            }),
        {
            enabled: !!parentId && !!name,
        }
    );
}
