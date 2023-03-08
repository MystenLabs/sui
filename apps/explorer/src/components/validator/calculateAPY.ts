// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Validator } from '@mysten/sui.js';

import { roundFloat } from '~/utils/roundFloat';

const APY_DECIMALS = 4;

// TODO: share code with `calculateAPY` for wallet?
export function calculateAPY(validator: Validator, epoch: number) {
    let apy;
    const { sui_balance, activation_epoch, pool_token_balance } =
        validator.staking_pool;

    // If the staking pool is active then we calculate its APY.
    if (activation_epoch.vec.length > 0) {
        const num_epochs_participated = +epoch - +activation_epoch.vec[0];
        apy =
            Math.pow(
                1 + (+sui_balance - +pool_token_balance) / +pool_token_balance,
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
