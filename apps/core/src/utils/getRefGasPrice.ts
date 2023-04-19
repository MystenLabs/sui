// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiValidatorSummary } from '@mysten/sui.js';

const REF_THRESHOLD = 66.67;

/**
 * Util to get the Reference Gas Price from a list of validators
 * 1. Sort validators by gas price
 * 2. Calculate the stake threshold at 66.67% of total stake amount
 * 3. Add up the stake amount of each validator until the threshold is reached
 * 4. Return the gas price of the last validator that was added to the sum
 */
export function getRefGasPrice(validators?: SuiValidatorSummary[]) {
    if (!validators?.length) {
        return '0';
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

    const sumAtThreshold =
        (totalStaked * BigInt(Math.round(REF_THRESHOLD))) / BigInt(100);

    let sumOfStakes = BigInt(0);
    let result = '0';

    for (let i = 0; i < sortedByGasPrice.length; i++) {
        const validator = sortedByGasPrice[i];

        const currentGasPrice = validator.gasPrice;
        const stake = BigInt(validator?.stakingPoolSuiBalance);

        sumOfStakes += stake;
        result = currentGasPrice;

        if (sumOfStakes >= sumAtThreshold) {
            break;
        }
    }

    return result;
}
