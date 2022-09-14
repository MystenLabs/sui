// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useIntl } from 'react-intl';

import type { IntlShape } from 'react-intl';

type Mode = Parameters<typeof Coin.getFormatData>['2'];

export function coinFormat(
    intl: IntlShape,
    balance: bigint,
    coinTypeArg: string,
    mode: Mode
) {
    const { value, formatOptions, symbol, forcedFormatValue } =
        Coin.getFormatData(balance, coinTypeArg, mode);
    const displayBalance =
        forcedFormatValue || intl.formatNumber(value, formatOptions);
    return {
        displayBalance,
        symbol,
        displayFull: forcedFormatValue || `${displayBalance} ${symbol}`,
    };
}

export function useCoinFormat(
    balance: bigint,
    coinTypeArg: string,
    mode: Mode
) {
    const intl = useIntl();
    return useMemo(
        () => coinFormat(intl, balance, coinTypeArg, mode),
        [intl, balance, coinTypeArg, mode]
    );
}
