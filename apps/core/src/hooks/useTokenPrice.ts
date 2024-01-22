// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';

import { useAppsBackend } from './useAppsBackend';
import { useCoinMetadata } from './useFormatCoin';
import { useSuiCoinData } from './useSuiCoinData';

type TokenPriceResponse = { price: string | null };

export function useTokenPrice(coinType: string) {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'token-price', coinType],
		queryFn: () => request<TokenPriceResponse>(`cetus/${coinType}`),

		// These values are set to one minute to prevent displaying stale data, as token prices can change frequently.
		staleTime: 60 * 1000,
		gcTime: 60 * 1000,
	});
}

export function useBalanceInUSD(coinType: string, balance: bigint | string | number) {
	const { data: coinMetadata } = useCoinMetadata(coinType);
	const { data: tokenPrice } = useTokenPrice(coinType);
	const { data: suiCoinData } = useSuiCoinData();
	if (!tokenPrice || !coinMetadata || !tokenPrice.price) return null;

	// we have a wallet requirement to source Sui price info from CoinGecko instead of Cetus
	const price = coinType === SUI_TYPE_ARG ? suiCoinData?.currentPrice ?? 0 : tokenPrice.price;
	const formattedBalance = new BigNumber(balance.toString()).shiftedBy(-1 * coinMetadata.decimals);

	return formattedBalance.multipliedBy(price).toNumber();
}
