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
        delegation.delegationStatus === 'Pending'
    )
        return 0;
    const pool_id = delegation.stakedSui.poolId;
    const validator = activeValidators.find(
        (validator) => validator.stakingPoolId === pool_id
    );

    if (!validator) return 0;

    const poolTokens = new BigNumber(
        delegation.delegationStatus.Active.poolTokens.value
    );
    const delegationTokenSupply = new BigNumber(validator.poolTokenBalance);
    const suiBalance = new BigNumber(validator.stakingPoolSuiBalance);
    const pricipalAmout = new BigNumber(
        delegation.delegationStatus.Active.principalSuiAmount
    );
    const currentSuiWorth = poolTokens
        .multipliedBy(suiBalance)
        .dividedBy(delegationTokenSupply);

    const earnToken = currentSuiWorth.minus(pricipalAmout);
    return earnToken.decimalPlaces(0, BigNumber.ROUND_DOWN).toNumber();
}
