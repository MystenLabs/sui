// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Validator } from '@mysten/sui.js';

import { roundFloat } from '_helpers';

const APY_DECIMALS = 4;

export function calculateAPY(validators: Validator, epoch: number) {
    const { sui_balance, starting_epoch, pool_token_balance } =
        validators.delegation_staking_pool;

    const num_epochs_participated = +epoch - +starting_epoch;
    const apy =
        Math.pow(
            1 + (+sui_balance - +pool_token_balance) / +pool_token_balance,
            365 / num_epochs_participated
        ) - 1;

    //guard against NaN
    const apyReturn = apy ? roundFloat(apy, APY_DECIMALS) : 0;

    // guard against very large numbers (e.g. 1e+100)
    return apyReturn > 100_000 ? 0 : apyReturn;
}
