// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

export const getStakedPercent = (
    stake: bigint | string,
    total: bigint
): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    const bnPercent = bnStake
    .div(bnTotal)
    .multipliedBy(100)
    .decimalPlaces(3, BigNumber.ROUND_DOWN)
    .toNumber()
    return bnPercent || 0;
};
