// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import useAppSelector from './useAppSelector';
import { accountItemizedBalancesSelector } from '_redux/slices/account';

export function useIndividualCoinMaxBalance(coinTypeArg: string) {
    const allCoins = useAppSelector(accountItemizedBalancesSelector);
    const maxGasCoinBalance = useMemo(
        () =>
            allCoins[coinTypeArg]?.reduce(
                (max, aBalance) => (max < aBalance ? aBalance : max),
                BigInt(0)
            ) || BigInt(0),
        [allCoins, coinTypeArg]
    );
    return maxGasCoinBalance;
}
