// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

import type { SuiValidatorSummary, StakeObject } from '@mysten/sui.js';

export function getStakingRewards(
    validator: SuiValidatorSummary,
    stakes: StakeObject
) {
    if (!validator || !stakes || stakes.status === 'Pending') return 0;

    if (!validator) return 0;

    const poolTokens = new BigNumber(stakes.principal);
    const delegationTokenSupply = new BigNumber(validator.poolTokenBalance);
    const suiBalance = new BigNumber(validator.stakingPoolSuiBalance);
    const principalAmount = new BigNumber(stakes.principal);

    const currentSuiWorth = poolTokens
        .multipliedBy(suiBalance)
        .dividedBy(delegationTokenSupply);

    const earnToken = currentSuiWorth.minus(principalAmount);
    return earnToken.decimalPlaces(0, BigNumber.ROUND_DOWN).toNumber();
}
