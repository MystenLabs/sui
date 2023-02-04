// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

import type { FormatNumberOptions } from 'react-intl';

export const percentageFormatOptions: FormatNumberOptions = {
    style: 'percent',
    maximumFractionDigits: 2,
    minimumFractionDigits: 0,
};

export const formatPercentage = (
    num1: bigint | string | number,
    num2: bigint | string | number,
    decimalPlaces = 3
) => {
    const bn1 = new BigNumber(num1.toString());
    const bn2 = new BigNumber(num2.toString());
    const percentage = bn1
        .div(bn2)
        .multipliedBy(100)
        .decimalPlaces(decimalPlaces)
        .toNumber();
    return percentage;
};
