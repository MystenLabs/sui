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

export const mainnetPools = {
	SUI_USDC_1: '0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be',
	SUI_USDC_2: '0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7', // not working currently
	WETH_USDC_1: '0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826',
	TBTC_USDC_1: '0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899',
};

export const coinsMap = {
	SUI: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
	USDC: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
	USDT: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
	WETH: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
	tBTC: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
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

export function useDeepbookPrices(poolId: keyof typeof mainnetPools, side: 'ask' | 'bid' = 'ask') {
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

function useAverageSuiPrice() {
	const { data: suiPrices } = useDeepbookPrices('SUI_USDC_2');
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
