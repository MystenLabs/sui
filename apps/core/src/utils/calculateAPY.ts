// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiValidatorSummary } from '@mysten/sui.js';
import { roundFloat } from './roundFloat';

const DEFAULT_APY_DECIMALS = 4;

export function calculateAPY(
    validator: SuiValidatorSummary,
    epoch: number,
    roundDecimals = DEFAULT_APY_DECIMALS
) {
    let apy;
    const {
        stakingPoolSuiBalance,
        stakingPoolActivationEpoch,
        poolTokenBalance,
    } = validator;

    // If the staking pool is active then we calculate its APY. Or if staking started in epoch 0
    if (stakingPoolActivationEpoch || stakingPoolActivationEpoch === 0) {
        const numEpochsParticipated = epoch - stakingPoolActivationEpoch;
        apy =
            (Math.pow(
                1 +
                    (+stakingPoolSuiBalance - +poolTokenBalance) /
                        +poolTokenBalance,
                365 / numEpochsParticipated
            ) -
                1) *
            100;
    } else {
        apy = 0;
    }

    //guard against NaN and large numbers
    return Number.isNaN(apy) || apy > 100_000
        ? 0
        : roundFloat(apy, roundDecimals);
}
