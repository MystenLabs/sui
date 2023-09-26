// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getDeepbookClient } from '_shared/deepbook-client';
import { SUI_DECIMALS } from '@mysten/sui.js/utils';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const FLOAT_SCALING_FACTOR = 1_000_000_000n;
export const DEFAULT_TICK_SIZE = 1n * FLOAT_SCALING_FACTOR;
const DEEPBOOK_KEY = 'deepbook';
export const SUI_DIVISOR = 1_000_000;

export enum PRICES {
	SUI_USDC_1 = 'SUI_USDC_1',
	SUI_USDC_2 = 'SUI_USDC_2',
	WETH_USDC_1 = 'WETH_USDC_1',
	TBTC_USDC_1 = 'TBTC_USDC_1',
}

export const mainnetPools = {
	[PRICES.SUI_USDC_1]: '0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be',
	[PRICES.SUI_USDC_2]: '0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7', // not working currently
	[PRICES.WETH_USDC_1]: '0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826',
	[PRICES.TBTC_USDC_1]: '0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899',
};

export function useDeepbookPools() {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: async () => {
			const deepbookClient = await getDeepbookClient();
			return deepbookClient.getAllPools({});
		},
		select: ({ data }) => data,
	});
}

export function useDeepbookPrices(poolId: PRICES, side: 'ask' | 'bid' = 'ask') {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices', poolId, side],
		queryFn: async () => {
			const deepbookClient = await getDeepbookClient();
			return deepbookClient.getLevel2BookStatus(
				mainnetPools[poolId],
				BigInt(0),
				// TODO: need to switch back to 10n * DEFAULT_TICK_SIZE
				// 10n * DEFAULT_TICK_SIZE,
				10000n,
				side,
			);
		},
		enabled: !!poolId && !!mainnetPools[poolId],
	});
}

export function useAverageSuiPrice() {
	const { data: suiPrices } = useDeepbookPrices(PRICES.SUI_USDC_2);
	return useMemo(() => {
		if (!suiPrices) return null;

		const totalPrice = suiPrices.reduce((acc: bigint, { price }: { price: bigint }) => {
			return acc + price;
		}, 0n);

		return new BigNumber(totalPrice.toString())
			.dividedBy(suiPrices.length)
			.dividedBy(SUI_DIVISOR)
			.toNumber();
	}, [suiPrices]);
}

export function useSuiBalanceInUSDC(suiBalance: BigInt | BigNumber) {
	const averageSuiPrice = useAverageSuiPrice();

	return useMemo(() => {
		if (!averageSuiPrice || !suiBalance) return null;

		const walletBalanceInSui = new BigNumber(suiBalance.toString())
			.shiftedBy(-1 * SUI_DECIMALS)
			.toNumber();

		return walletBalanceInSui * averageSuiPrice;
	}, [averageSuiPrice, suiBalance]);
}
