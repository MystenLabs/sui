// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiValidatorSummary } from '@mysten/sui.js';

import { roundFloat } from '~/utils/roundFloat';

const APY_DECIMALS = 4;

// TODO: share code with `calculateAPY` for wallet?
export function calculateAPY(validator: SuiValidatorSummary, epoch: number) {
    let apy;
    const {
        staking_pool_sui_balance,
        staking_pool_activation_epoch,
        pool_token_balance,
    } = validator;

    // If the staking pool is active then we calculate its APY.
    if (staking_pool_activation_epoch) {
        const num_epochs_participated = +epoch - +staking_pool_activation_epoch;
        apy =
            Math.pow(
                1 +
                    (+staking_pool_sui_balance - +pool_token_balance) /
                        +pool_token_balance,
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
