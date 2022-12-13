// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { useGetObject } from './useGetObject';

//TODO: Remove when available on SDK, types should come from the SDK
import type {
    Validator,
    ValidatorState,
} from '_app/staking/home/ValidatorDataTypes';

const VALIDATORS_OBJECT_ID = '0x05';

function processValidators(
    set: Validator[],
    totalStake: bigint,
    current_epoch: number,
    walletAddress?: string | null
) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;

        const name = Buffer.from(rawName, 'base64').toString();

        const {
            sui_balance,
            starting_epoch,
            pending_delegations,
            delegation_token_supply,
        } = av.fields.delegation_staking_pool.fields;
        const num_epochs_participated = current_epoch - starting_epoch;
        const APY =
            (1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value) ^
            (365 / num_epochs_participated - 1);

        const pending_delegationsByAddress = pending_delegations
            ? pending_delegations.filter(
                  (d) => d.fields.delegator === walletAddress
              )
            : [];

        return {
            name: name,
            address: av.fields.metadata.fields.sui_address,
            stake: av.fields.stake_amount,
            stakePercent: getStakePercent(av.fields.stake_amount, totalStake),
            commissionRate: av.fields.commission_rate || 0,
            delegationCount: av.fields.delegation_count || 0,
            apy: APY > 0 ? APY : 'N/A',

            amount: av.fields.stake_amount || 0n,
            // only show pending delegation addreeses if there is a pending delegation
            pendingDelegations: pending_delegationsByAddress,
            pendingDelegationAmount: pending_delegationsByAddress.reduce(
                (acc, fields) =>
                    (acc += BigInt(fields.fields.sui_amount || 0n)),
                0n
            ),
            metadata: av.fields.metadata,
            // TODO: update
            pendingDelegationsCount: pending_delegations.length,
            totalPendingDelegationAmount: BigInt(
                av.fields.metadata.fields.next_epoch_delegation || 0n
            ),
            logo: null,
            suiEarned: 0n,
        };
    });
}

function getStakePercent(stake: bigint, total: bigint): number {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
}

export function useGetValidators(walletAddress: string | null) {
    // TODO add cache invalidation to useGetObject. Prevents stale data  after staking and destaking
    const { data, isLoading, isError } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const totalStake =
        validatorsData?.validators.fields.total_validator_stake || 0n;

    const validators = useMemo(() => {
        if (!validatorsData) return [];
        const processedValidators = processValidators(
            validatorsData.validators.fields.active_validators,
            totalStake,
            validatorsData.epoch,
            walletAddress
        );

        return processedValidators.sort((a, b) => (a.name > b.name ? 1 : -1));
    }, [totalStake, validatorsData, walletAddress]);
    return { validators, isLoading, isError };
}
