// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

<<<<<<< HEAD
import { Coin } from '@mysten/sui.js';
=======
import { Coin, JsonRpcProvider } from '@mysten/sui.js';
>>>>>>> 5a6d088b8 (only include core changes)
import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useRpcClient } from '../api/RpcClientContext';
<<<<<<< HEAD
=======

>>>>>>> 5a6d088b8 (only include core changes)
import { formatAmount } from '../utils/formatAmount';

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
    const bn = new BigNumber(balance.toString()).shiftedBy(-1 * decimals);

    if (format === CoinFormat.FULL) {
        return bn.toFormat();
    }

    return formatAmount(bn);
}

export function useCoinDecimals(
    coinType?: string | null
) {
    const rpc = useRpcClient();
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

// TODO #1: This handles undefined values to make it easier to integrate with
// the reset of the app as it is today, but it really shouldn't in a perfect world.
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
    const { isFetched } = queryResult;

    const formatted = useMemo(() => {
        if (typeof balance === 'undefined' || balance === null) return '';

        if (!isFetched) return '...';

        return formatBalance(balance, decimals, format);
    }, [decimals, isFetched, balance, format]);

    return [formatted, symbol, queryResult];
}
