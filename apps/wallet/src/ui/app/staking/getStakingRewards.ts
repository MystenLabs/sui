// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

import type { SuiValidatorSummary, DelegatedStake } from '@mysten/sui.js';

export function getStakingRewards(
    activeValidators: SuiValidatorSummary[],
    delegation: DelegatedStake
) {
    if (
        !activeValidators ||
        !delegation ||
        delegation.delegation_status === 'Pending'
    )
        return 0;
    const pool_id = delegation.staked_sui.pool_id;
    const validator = activeValidators.find(
        (validator) => validator.staking_pool_id === pool_id
    );

    if (!validator) return 0;

    const poolTokens = new BigNumber(
        delegation.delegation_status.Active.pool_tokens.value
    );
    const delegationTokenSupply = new BigNumber(validator.pool_token_balance);
    const suiBalance = new BigNumber(validator.staking_pool_sui_balance);
    const pricipalAmout = new BigNumber(
        delegation.delegation_status.Active.principal_sui_amount
    );
    const currentSuiWorth = poolTokens
        .multipliedBy(suiBalance)
        .dividedBy(delegationTokenSupply);

    const earnToken = currentSuiWorth.minus(pricipalAmout);
    return earnToken.decimalPlaces(0, BigNumber.ROUND_DOWN).toNumber();
}
