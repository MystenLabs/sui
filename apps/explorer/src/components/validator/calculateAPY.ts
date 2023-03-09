// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Validator } from '@mysten/sui.js';

import { roundFloat } from '~/utils/roundFloat';

const APY_DECIMALS = 4;

// TODO: share code with `calculateAPY` for wallet?
export function calculateAPY(validator: Validator, epoch: number) {
    let apy;
    const { suiBalance, activationEpoch, poolTokenBalance } =
        validator.stakingPool;

    // If the staking pool is active then we calculate its APY.
    if (activationEpoch.vec.length > 0) {
        const num_epochs_participated = +epoch - +activationEpoch.vec[0];
        apy =
            Math.pow(
                1 + (+suiBalance - +poolTokenBalance) / +poolTokenBalance,
                365 / num_epochs_participated
            ) - 1;
    } else {
        apy = 0;
    }

    //guard against NaN
    const apyReturn = apy ? roundFloat(apy, APY_DECIMALS) : 0;

    // guard against very large numbers (e.g. 1e+100)
    return apyReturn > 100_000 ? 0 : apyReturn;
}
