// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ActiveValidator } from '@mysten/sui.js';

const APY_DECIMALS = 4;

export function calculateAPY(validators: ActiveValidator, epoch: number) {
    const { sui_balance, starting_epoch, delegation_token_supply } =
        validators.fields.delegation_staking_pool.fields;

    const num_epochs_participated = +epoch - +starting_epoch;
    const apy = Math.pow(
        1 +
            (+sui_balance - +delegation_token_supply.fields.value) /
                +delegation_token_supply.fields.value,
        365 / num_epochs_participated - 1
    );
    return apy ? parseFloat(apy.toFixed(APY_DECIMALS)) : 0;
}
