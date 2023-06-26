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
	queryResult: UseQueryResult<CoinMetadata | null>,
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
	format: CoinFormat = CoinFormat.ROUNDED,
) {
	const bn = new BigNumber(balance.toString()).shiftedBy(-1 * decimals);

	if (format === CoinFormat.FULL) {
		return bn.toFormat();
	}

	return formatAmount(bn);
}

const ELLIPSIS = '\u{2026}';
const SYMBOL_TRUNCATE_LENGTH = 5;
const NAME_TRUNCATE_LENGTH = 10;

export function useCoinMetadata(coinType?: string | null) {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['coin-metadata', coinType],
		queryFn: async () => {
			if (!coinType) {
				throw new Error('Fetching coin metadata should be disabled when coin type is disabled.');
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
		select(data) {
			if (!data) return null;

			return {
				...data,
				symbol:
					data.symbol.length > SYMBOL_TRUNCATE_LENGTH
						? data.symbol.slice(0, SYMBOL_TRUNCATE_LENGTH) + ELLIPSIS
						: data.symbol,
				name:
					data.name.length > NAME_TRUNCATE_LENGTH
						? data.name.slice(0, NAME_TRUNCATE_LENGTH) + ELLIPSIS
						: data.name,
			};
		},
		retry: false,
		enabled: !!coinType,
		staleTime: Infinity,
		cacheTime: 24 * 60 * 60 * 1000,
	});
}

// TODO #1: This handles undefined values to make it easier to integrate with
// the reset of the app as it is today, but it really shouldn't in a perfect world.
export function useFormatCoin(
	balance?: bigint | number | string | null,
	coinType?: string | null,
	format: CoinFormat = CoinFormat.ROUNDED,
): FormattedCoin {
	const fallbackSymbol = useMemo(() => (coinType ? Coin.getCoinSymbol(coinType) : ''), [coinType]);

	const queryResult = useCoinMetadata(coinType);
	const { isFetched, data } = queryResult;

	const formatted = useMemo(() => {
		if (typeof balance === 'undefined' || balance === null) return '';

		if (!isFetched) return '...';

		return formatBalance(balance, data?.decimals ?? 0, format);
	}, [data?.decimals, isFetched, balance, format]);

	return [formatted, isFetched ? data?.symbol || fallbackSymbol : '', queryResult];
}
