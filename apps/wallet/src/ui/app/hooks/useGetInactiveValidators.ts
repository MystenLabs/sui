// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type ObjectId } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { notEmpty } from '_helpers';

// Get inActive validators by inactivePoolsIds
export function useGetInactiveValidators(inactivePoolsId?: ObjectId) {
    const rpc = useRpcClient();
    const data = useQuery(
        ['inactive-pool-id', inactivePoolsId],
        () => rpc.getDynamicFields({ parentId: inactivePoolsId! }),
        { enabled: !!inactivePoolsId }
    );
    // return validator stakePoolId
    return {
        ...data,
        data:
            data.data?.data
                .map((validator) => validator.name?.value || null)
                .filter(notEmpty) || [],
    };
}
