// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
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
        ['dynamic-fields-object', parentId],
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
