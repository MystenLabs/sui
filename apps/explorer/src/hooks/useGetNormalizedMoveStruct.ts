// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { normalizeSuiObjectId } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetNormalizedMoveStruct(
    packageId: string,
    module: string,
    struct: string
) {
    const rpc = useRpcClient();
    return useQuery(
        ['normalized-struct', packageId, module, struct],
        () =>
            rpc.getNormalizedMoveStruct({
                package: normalizeSuiObjectId(packageId),
                module,
                struct,
            }),
        { enabled: !!packageId && !!module && !!struct }
    );
}
