// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ActiveValidator, DelegatedStake } from '@mysten/sui.js';

export function getEarnToken(
    activeValidators: ActiveValidator[],
    delegation: DelegatedStake
) {
    if (
        !activeValidators ||
        !delegation ||
        delegation.delegation_status === 'Pending'
    )
        return 0;
    const validatorAddress = delegation.staked_sui.validator_address;
    const validator = activeValidators.find(
        ({ fields }) =>
            fields.delegation_staking_pool.fields.validator_address ===
            validatorAddress
    );

    if (!validator) return 0;
    const { fields: validatorFields } = validator;

    const poolTokens = delegation.delegation_status.Active.pool_tokens.value;
    const delegationTokenSupply =
        validatorFields.delegation_staking_pool.fields.delegation_token_supply
            .fields.value;
    const suiBlance =
        validatorFields.delegation_staking_pool.fields.sui_balance;
    const currentSuiWorth = (poolTokens * +suiBlance) / +delegationTokenSupply;
    const earnToken =
        currentSuiWorth -
        delegation.delegation_status.Active.principal_sui_amount;
    return earnToken > 0 ? earnToken : 0;
    // return list
}
