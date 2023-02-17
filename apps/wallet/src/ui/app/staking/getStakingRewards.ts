// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

import type { MoveActiveValidator, DelegatedStake } from '@mysten/sui.js';

export function getStakingRewards(
    activeValidators: MoveActiveValidator[],
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

    const poolTokens = new BigNumber(
        delegation.delegation_status.Active.pool_tokens.value
    );
    const delegationTokenSupply = new BigNumber(
        validatorFields.delegation_staking_pool.fields.delegation_token_supply.fields.value
    );
    const suiBalance = new BigNumber(
        validatorFields.delegation_staking_pool.fields.sui_balance
    );
    const pricipalAmout = new BigNumber(
        delegation.delegation_status.Active.principal_sui_amount
    );
    const currentSuiWorth = poolTokens
        .multipliedBy(suiBalance)
        .dividedBy(delegationTokenSupply);

    const earnToken = currentSuiWorth.minus(pricipalAmout);
    return earnToken.decimalPlaces(0, BigNumber.ROUND_DOWN).toNumber();
}
