// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

// NOTE Temporary  until SUI SDK is updated
// TODO: add to SUI SDK once Validator types is finalized
// Get validators by account address
/**
 *
 * @see {@link https://github.com/MystenLabs/sui/blob/b904dede65c91c112434d49180e2d277e76ccee6/crates/sui-types/src/sui_system_state.rs#L42}
 *
 */

const STAKE_DELEGATOR_STALE_TIME = 5 * 1000;

export type ValidatorMetaData = {
    sui_address: SuiAddress;
    pubkey_bytes: number[];
    network_pubkey_bytes: number[];
    worker_pubkey_bytes: number[];
    proof_of_possession_bytes: number[];
    name: number[];
    net_address: number[];
    consensus_address: number[];
    worker_address: number[];
    next_epoch_stake: number;
    next_epoch_delegation: number;
    next_epoch_gas_price: number;
    next_epoch_commission_rate: number;
};

// Staking
type Id = {
    id: string;
};

type Balance = {
    value: bigint;
};

type StakedSui = {
    id: Id;
    validator_address: SuiAddress;
    pool_starting_epoch: bigint;
    delegation_request_epoch: bigint;
    principal: Balance;
    sui_token_lock: bigint | null;
};

type ActiveDelegationStatus = {
    Active: {
        id: Id;
        staked_sui_id: SuiAddress;
        principal_sui_amount: bigint;
        pool_tokens: Balance;
    };
};

export type DelegatedStake = {
    staked_sui: StakedSui;
    delegation_status: 'Pending' | ActiveDelegationStatus;
};

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
    return res.result as DelegatedStake[];
};

export function useGetValidatorsByDelegator(
    address: string
): UseQueryResult<DelegatedStake[], Error> {
    const rpcEndPoint = useRpc().endpoints.fullNode;
    return useQuery(
        ['delegated-staked-data', address],
        async () => getDelegatedStakes(address, rpcEndPoint),
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
    return useQuery(['validator-meta-data'], async () => {
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
        return res.result as ValidatorMetaData[];
    });
}
