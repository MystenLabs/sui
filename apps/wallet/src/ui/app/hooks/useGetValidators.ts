// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { useGetObject } from './useGetObject';
import {
    STATE_OBJECT,
    VALDIATOR_NAME,
} from '_app/staking/usePendingDelegation';

//TODO: Remove when available on SDK, types should come from the SDK
import type { ValidatorState } from '_app/staking/home/ValidatorDataTypes';

function getStakePercent(stake: bigint, total: bigint): number {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
}

export function useGetValidators(walletAddress: string | null) {
    const { data, isLoading, isError } = useGetObject(STATE_OBJECT);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const totalStake =
        validatorsData?.validators.fields.total_validator_stake || 0n;

    const validators = useMemo(() => {
        if (!validatorsData) return [];
        return validatorsData.validators.fields.active_validators.map((av) => {
            const rawName = av.fields.metadata.fields.name;

            let name: string;

            if (Array.isArray(rawName)) {
                name = String.fromCharCode(...rawName);
            } else {
                name = Buffer.from(rawName, 'base64').toString();
                if (!VALDIATOR_NAME.test(name)) {
                    name = rawName;
                }
            }
            const {
                sui_balance,
                starting_epoch,
                pending_delegations,
                delegation_token_supply,
            } = av.fields.delegation_staking_pool.fields;

            const num_epochs_participated =
                validatorsData.epoch - starting_epoch;
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
                apy: APY > 0 ? APY : 'N/A',
                logo: null,
                stakePercent: getStakePercent(
                    av.fields.stake_amount,
                    totalStake
                ),
                pendingDelegationAmount: pending_delegationsByAddress.reduce(
                    (acc, fields) =>
                        (acc += BigInt(fields.fields.sui_amount || 0n)),
                    0n
                ),
                av,
            };
        });
    }, [totalStake, validatorsData, walletAddress]);
    return { validators, isLoading, isError };
}
