// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useIntl } from 'react-intl';

import { FEATURES } from '../experimentation/features';
import { Coin } from '../redux/slices/sui-objects/Coin';
import { api } from '../redux/store/thunk-extras';

type FormattedCoin = [formattedBalance: string, coinSymbol: string];

export function formatBalance(
    balance: bigint | number | string,
    decimals: number
) {
    const bn = new BigNumber(balance.toString()).shiftedBy(-1 * decimals);

    return bn.toFormat(bn.gte(1) ? 2 : undefined);
}

export function useCoinDecimals(coinType?: string | null) {
    const suiDenomination = useFeature(FEATURES.SUI_DENOMINATION).on;

    const queryResult = useQuery(
        ['denomination', coinType],
        async () => {
            if (!coinType) {
                throw new Error(
                    'Fetching coin denomination should be disabled when coin type is disabled.'
                );
            }

            return api.instance.fullNode.getCoinDenominationInfo(coinType);
        },
        {
            // This is currently expected to fail for non-SUI tokens, so disable retries:
            retry: false,
            enabled: suiDenomination && !!coinType,
            // Never consider this data to be stale:
            staleTime: Infinity,
            // Keep this data in the cache for 24 hours.
            // We allow this to be GC'd after a very long time to avoid unbounded cache growth.
            cacheTime: 24 * 60 * 60 * 1000,
        }
    );

    return [queryResult.data?.decimalNumber || 0, queryResult] as const;
}

// TODO: This handles undefined values to make it easier to integrate with the reset of the app as it is
// today, but it really shouldn't in a perfect world.
export function useFormatCoin(
    balance?: bigint | number | string | null,
    coinType?: string | null
): FormattedCoin {
    const intl = useIntl();
    const suiDenomination = useFeature(FEATURES.SUI_DENOMINATION).on;
    const symbol = useMemo(
        () => (coinType ? Coin.getCoinSymbol(coinType) : ''),
        [coinType]
    );

    const [decimals, { isFetched, isError }] = useCoinDecimals(coinType);

    const formatted = useMemo(() => {
        if (!balance) return '';

        if (!suiDenomination || isError) {
            return intl.formatNumber(BigInt(balance), {
                maximumFractionDigits: 0,
            });
        }

        // TODO: Figure out more ideal loading state:
        if (!isFetched) return '...';

        return formatBalance(balance, decimals);
    }, [decimals, isError, isFetched, suiDenomination, intl, balance]);

    return [formatted, symbol];
}
