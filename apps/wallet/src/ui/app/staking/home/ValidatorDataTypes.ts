// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';

//TODO pull from the SDK

export type ValidatorMetadata = {
    type: '0x2::validator::ValidatorMetadata';
    fields: {
        name: string;
        net_address: string;
        next_epoch_stake: number;
        pubkey_bytes: string;
        sui_address: string;
        next_epoch_delegation: number;
    };
};

export type Validator = {
    type: '0x2::validator::Validator';
    fields: {
        delegation: bigint;
        delegation_count: number;
        metadata: ValidatorMetadata;
        pending_delegation: bigint;
        pending_delegation_withdraw: bigint;
        pending_delegator_count: number;
        pending_delegator_withdraw_count: number;
        pending_stake: {
            type: '0x1::option::Option<0x2::balance::Balance<0x2::sui::SUI>>';
            fields: any;
        };
        commission_rate: number;
        pending_withdraw: bigint;
        stake_amount: bigint;
        delegation_staking_pool: {
            fields: {
                sui_balance: number;
                starting_epoch: number;
                delegation_token_supply: {
                    type: string;
                    fields: {
                        value: number;
                    };
                };
            };
        };
    };
};

export type SystemParams = {
    type: '0x2::sui_system::SystemParameters';
    fields: {
        max_validator_candidate_count: number;
        min_validator_stake: bigint;
    };
};

export type ValidatorState = {
    delegation_reward: number;
    epoch: number;
    id: { id: string; version: number };
    parameters: SystemParams;
    storage_fund: number;
    validators: {
        type: '0x2::validator_set::ValidatorSet';
        fields: {
            delegation_stake: bigint;
            active_validators: Validator[];
            next_epoch_validators: Validator[];
            pending_removals: string;
            pending_validators: string;
            quorum_stake_threshold: bigint;
            total_validator_stake: bigint;
        };
    };
};

const textDecoder = new TextDecoder();

export function processValidators(
    set: Validator[],
    totalStake: bigint,
    current_epoch: number
) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;

        const name = textDecoder.decode(
            new Base64DataBuffer(rawName).getData()
        );

        const {
            sui_balance,
            starting_epoch,

            delegation_token_supply,
        } = av.fields.delegation_staking_pool.fields;
        const num_epochs_participated = current_epoch - starting_epoch;
        const APY =
            (1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value) ^
            (365 / num_epochs_participated - 1);

        return {
            name: name,
            address: av.fields.metadata.fields.sui_address,
            pubkeyBytes: av.fields.metadata.fields.pubkey_bytes,
            stake: av.fields.stake_amount,
            stakePercent: getStakePercent(av.fields.stake_amount, totalStake),
            delegation_count: av.fields.delegation_count || 0,
            apy: APY > 0 ? APY : 'N/A',
            logo: null,
        };
    });
}

export const getStakePercent = (stake: bigint, total: bigint): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
};
