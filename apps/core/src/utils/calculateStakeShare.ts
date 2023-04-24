// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import BigNumber from 'bignumber.js';

export const calculateStakeShare = (
    validatorStake: bigint,
    totalStake: bigint,
    decimalPlaces = 2
) => {
    const bn = new BigNumber(validatorStake.toString());
    const bd = new BigNumber(totalStake.toString());
    const percentage = bn
        .div(bd)
        .multipliedBy(100)
        .decimalPlaces(decimalPlaces)
        .toNumber();
    return percentage;
};
