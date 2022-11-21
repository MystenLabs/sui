// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { useRpc } from './useRpc';

type FormattedCoin = [
    formattedBalance: string,
    coinSymbol: string,
    queryResult: UseQueryResult
];

export enum CoinFormat {
    ROUNDED = 'ROUNDED',
    FULL = 'FULL',
}

/**
 * Formats a coin balance based on our standard coin display logic.
 * If the balance is less than 1, it will be displayed in its full decimal form.
 * For values greater than 1, it will be truncated to 3 decimal places.
 */
export function formatBalance(
    balance: bigint | number | string,
    decimals: number,
    format: CoinFormat = CoinFormat.ROUNDED
) {
    let postfix = '';
    let bn = new BigNumber(balance.toString()).shiftedBy(-1 * decimals);

    if (format === CoinFormat.FULL) {
        return bn.toFormat();
    }

    if (bn.gte(1_000_000_000)) {
        bn = bn.shiftedBy(-9);
        postfix = ' B';
    } else if (bn.gte(1_000_000)) {
        bn = bn.shiftedBy(-6);
        postfix = ' M';
    } else if (bn.gte(10_000)) {
        bn = bn.shiftedBy(-3);
        postfix = ' K';
    }

    if (bn.gte(1)) {
        bn = bn.decimalPlaces(3, BigNumber.ROUND_DOWN);
    }

    return bn.toFormat() + postfix;
}

export function useCoinDecimals(coinType?: string | null) {
    const rpc = useRpc();

    const queryResult = useQuery(
        ['denomination', coinType],
        async () => {
            if (!coinType) {
                throw new Error(
                    'Fetching coin denomination should be disabled when coin type is disabled.'
                );
            }

            return rpc.getCoinMetadata(coinType);
        },
        {
            // This is currently expected to fail for non-SUI tokens, so disable retries:
            retry: false,
            enabled: !!coinType,
            // Never consider this data to be stale:
            staleTime: Infinity,
            // Keep this data in the cache for 24 hours.
            // We allow this to be GC'd after a very long time to avoid unbounded cache growth.
            cacheTime: 24 * 60 * 60 * 1000,
        }
    );

    return [queryResult.data?.decimals || 0, queryResult] as const;
}

const numberFormatter = new Intl.NumberFormat('en', {
    maximumFractionDigits: 0,
});

// TODO: Unify this into.
// Candidate holding packages:
// - @mysten/sui.js
// - @mysten/core (non-published helpers)
// - @mysten/coin (some new package that has helpers like this)
// TODO: This handles undefined values to make it easier to integrate with the reset of the app as it is
// today, but it really shouldn't in a perfect world.
export function useFormatCoin(
    balance?: bigint | number | string | null,
    coinType?: string | null,
    format: CoinFormat = CoinFormat.ROUNDED
): FormattedCoin {
    const symbol = useMemo(
        () => (coinType ? Coin.getCoinSymbol(coinType) : ''),
        [coinType]
    );

    const [decimals, queryResult] = useCoinDecimals(coinType);
    const { isFetched, isError } = queryResult;

    const formatted = useMemo(() => {
        if (typeof balance === 'undefined' || balance === null) return '';

        if (isError) {
            return numberFormatter.format(BigInt(balance));
        }

        if (!isFetched) return '...';

        return formatBalance(balance, decimals, format);
    }, [decimals, isError, isFetched, balance, format]);

    return [formatted, symbol, queryResult];
}
