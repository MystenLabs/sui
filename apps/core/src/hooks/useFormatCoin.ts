// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, CoinMetadata, SUI_TYPE_ARG } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useRpcClient } from '../api/RpcClientContext';
import { formatAmount } from '../utils/formatAmount';

type FormattedCoin = [
    formattedBalance: string,
    coinSymbol: string,
    queryResult: UseQueryResult<CoinMetadata | null>
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

export function useCoinMetadata(coinType?: string | null) {
    const rpc = useRpcClient();
    return useQuery(
        ['coin-metadata', coinType],
        async () => {
            if (!coinType) {
                throw new Error(
                    'Fetching coin metadata should be disabled when coin type is disabled.'
                );
            }

            // Optimize the known case of SUI to avoid a network call:
            if (coinType === SUI_TYPE_ARG) {
                const metadata: CoinMetadata = {
                    id: null,
                    decimals: 9,
                    description: '',
                    iconUrl: null,
                    name: 'Sui',
                    symbol: 'SUI',
                };

                return metadata;
            }

            return rpc.getCoinMetadata({ coinType });
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
}

// TODO #1: This handles undefined values to make it easier to integrate with
// the reset of the app as it is today, but it really shouldn't in a perfect world.
export function useFormatCoin(
    balance?: bigint | number | string | null,
    coinType?: string | null,
    format: CoinFormat = CoinFormat.ROUNDED
): FormattedCoin {
    const fallbackSymbol = useMemo(
        () => (coinType ? Coin.getCoinSymbol(coinType) : ''),
        [coinType]
    );

    const queryResult = useCoinMetadata(coinType);
    const { isFetched, data } = queryResult;

    const formatted = useMemo(() => {
        if (typeof balance === 'undefined' || balance === null) return '';

        if (!isFetched) return '...';

        return formatBalance(balance, data?.decimals ?? 0, format);
    }, [data?.decimals, isFetched, balance, format]);

    return [
        formatted,
        isFetched ? data?.symbol || fallbackSymbol : '',
        queryResult,
    ];
}
