// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

type StakePercent = {
    stake: bigint | string;
    total: bigint;
};
export const getStakedPercent = ({ stake, total }: StakePercent): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake
        .div(bnTotal)
        .multipliedBy(100)
        .decimalPlaces(3, BigNumber.ROUND_DOWN)
        .toNumber();
};
