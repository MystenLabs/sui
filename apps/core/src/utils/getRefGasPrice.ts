// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiValidatorSummary } from '@mysten/sui.js';
import { calculateStakeShare } from './calculateStakeShare';

const REF_THRESHOLD = 66.67;

/**
 * Util to get the Reference Gas Price from a list of validators
 * 1. Sort validators by gas price
 * 2. Add up stake share from low to high, until reaching REF_THRESHOLD
 * 3. Return the gas price of the last validator that was added to the sum
 */
export function getRefGasPrice(validators?: SuiValidatorSummary[]) {
    if (!validators?.length) {
        return BigInt(0);
    }

    const sortedByGasPrice = [...validators].sort((a, b) => {
        const aGasPrice = BigInt(a.gasPrice);
        const bGasPrice = BigInt(b.gasPrice);

        if (aGasPrice < bGasPrice) {
            return -1;
        }

        if (aGasPrice > bGasPrice) {
            return 1;
        }

        return 0;
    });

    const totalStaked = validators.reduce(
        (acc, cur) => acc + BigInt(cur.stakingPoolSuiBalance),
        BigInt(0)
    );

    let sumOfStakes = 0;
    let result = '0';

    for (let i = 0; i < sortedByGasPrice.length; i++) {
        const validator = sortedByGasPrice[i];
        const stake = BigInt(validator?.stakingPoolSuiBalance);

        const stakeShare = calculateStakeShare(stake, totalStaked);

        sumOfStakes += stakeShare;

        if (sumOfStakes >= REF_THRESHOLD) {
            result = validator.gasPrice;
            break;
        }
    }

    return BigInt(result);
}
