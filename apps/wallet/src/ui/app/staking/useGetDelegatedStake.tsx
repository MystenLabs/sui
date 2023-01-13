// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

import type { ValidatorMetaData, DelegatedStake } from '@mysten/sui.js';

const STAKE_DELEGATOR_STALE_TIME = 5 * 1000;

export function useGetDelegatedStake(
    address: string
): UseQueryResult<DelegatedStake[], Error> {
    const rpc = useRpc();
    return useQuery(
        ['validator', address],
        () => rpc.getDelegatedStake(address),
        {
            staleTime: STAKE_DELEGATOR_STALE_TIME,
        }
    );
}

// maybe be cached for a long time
export function useGetValidatorMetaData(): UseQueryResult<
    ValidatorMetaData[],
    Error
> {
    const rpc = useRpc();
    // keeping the query parent key the same to invalidate all related queries
    return useQuery(['validator', 'all'], async () => {
        return rpc.getValidators();
    });
}
