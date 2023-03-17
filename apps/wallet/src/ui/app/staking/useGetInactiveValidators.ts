// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type ObjectId } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

// Get invalid validators by inactivePoolsIds
// get system state summary
export function useGetInactiveValidators(inactivePoolsId?: ObjectId) {
    const rpc = useRpcClient();
    const data = useQuery(
        ['inactive-pool-id', inactivePoolsId],
        () => rpc.getDynamicFields({ parentId: inactivePoolsId! }),
        { enabled: !!inactivePoolsId }
    );

    return {
        ...data,
        // return validator address
        data:
            data.data?.data.map(
                (validator) => validator.name.value as string
            ) || [],
    };
}
