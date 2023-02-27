// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CoinStruct } from '@mysten/sui.js';

import { DEFAULT_GAS_BUDGET_FOR_PAY } from '_redux/slices/sui-objects/Coin';

// This is a helper function to get a set of gas coins that can cover a given amount
// It is from Coins.selectCoinSetWithCombinedBalanceGreaterThanOrEqual from sui.js
function getCoinSetWithCombinedBalanceGreaterThanOrEqual(
    coins: CoinStruct[],
    amount: bigint
) {
    // Sort coin by balance in an ascending order
    const sortedCoins = coins
        .filter(({ lockedUntilEpoch }) => !lockedUntilEpoch)
        .sort((a, b) => b.balance - a.balance);

    // calculate total balance
    const total = sortedCoins.reduce(
        (acc, { balance }) => acc + BigInt(balance),
        0n
    );
    if (total < amount) {
        return [];
    } else if (total === amount) {
        return sortedCoins;
    }

    let sum = BigInt(0);
    const ret = [];
    while (sum < total) {
        // prefer to add a coin with smallest sufficient balance
        const target = amount - sum;
        const coinWithSmallestSufficientBalance = sortedCoins.find(
            ({ balance }) => balance >= target
        );
        if (coinWithSmallestSufficientBalance) {
            ret.push(coinWithSmallestSufficientBalance);
            break;
        }

        const coinWithLargestBalance = sortedCoins.pop()!;
        ret.push(coinWithLargestBalance);
        sum += BigInt(coinWithLargestBalance.balance);
    }
    // sort coins by balance in ascending order
    return ret.sort((a, b) => a.balance - b.balance);
}

export function useGasBudgetEstimationUnits(
    coins: CoinStruct[] | null,
    amount: bigint
) {
    if (!coins) {
        return 0;
    }
    const numInputCoins = getCoinSetWithCombinedBalanceGreaterThanOrEqual(
        coins,
        amount
    ).length;
    return (
        DEFAULT_GAS_BUDGET_FOR_PAY *
        Math.max(2, Math.min(100, numInputCoins / 2))
    );
}
