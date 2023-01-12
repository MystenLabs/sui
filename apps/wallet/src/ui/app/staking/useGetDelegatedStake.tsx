// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

import type { ValidatorMetaData, DelegatedStake } from './ValidatorDataTypes';

const STAKE_DELEGATOR_STALE_TIME = 5 * 1000;

const getDelegatedStakes = async (
    address: SuiAddress,
    rpcEndPoint: string
): Promise<DelegatedStake[]> => {
    const response = await fetch(rpcEndPoint, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            method: 'sui_getDelegatedStakes',
            jsonrpc: '2.0',
            params: [address],
            id: 1,
        }),
    });

    if (!response.ok) {
        throw new Error(response.statusText);
    }

    const res = await response.json();
    if (!res?.result) {
        throw new Error(res.error.message);
    }
    return res.result as DelegatedStake[];
};

export function useGetDelegatedStake(
    address: string
): UseQueryResult<DelegatedStake[], Error> {
    const rpcEndPoint = useRpc().endpoints.fullNode;
    return useQuery(
        ['validator', address],
        () => getDelegatedStakes(address, rpcEndPoint),
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
    const rpcEndPoint = useRpc().endpoints.fullNode;
    return useQuery(['validator-1'], async () => {
        const response = await fetch(rpcEndPoint, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                method: 'sui_getValidators',
                jsonrpc: '2.0',
                id: 1,
            }),
        });
        if (!response.ok) {
            throw new Error(response.statusText);
        }
        const res = await response.json();

        if (!res?.result) {
            throw new Error(res.error.message);
        }
        return res.result as ValidatorMetaData[];
    });
}
