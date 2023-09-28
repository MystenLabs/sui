// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getDeepbookClient } from '_shared/deepbook-client';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const FLOAT_SCALING_FACTOR = 1_000_000_000n;
export const DEFAULT_TICK_SIZE = 1n * FLOAT_SCALING_FACTOR;
const DEEPBOOK_KEY = 'deepbook';
export const SUI_DIVISOR = 1_000_000;
export const USDT_DIVISOR = 1000 * SUI_DIVISOR;
export const OTHER_DIVISOR = 10 * SUI_DIVISOR;

export const mainnetPools = {
	SUI_USDC_1: '0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be',
	SUI_USDC_2: '0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7', // not working currently due to pagination
	WETH_USDC_1: '0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826',
	TBTC_USDC_1: '0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899',
	USDT_USDC_1: '0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955',
};

export enum Coins {
	SUI = 'SUI',
	USDC = 'USDC',
	USDT = 'USDT',
	WETH = 'WETH',
	tBTC = 'tBTC',
}

export const coinsMap = {
	[Coins.SUI]: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
	[Coins.USDC]: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
	[Coins.USDT]: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
	[Coins.WETH]: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
	[Coins.tBTC]: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
};

export function getUSDCurrency(amount: number | null) {
	if (typeof amount !== 'number') {
		return null;
	}

	return amount.toLocaleString('en', {
		style: 'currency',
		currency: 'USD',
	});
}

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

export function useDeepbookPrices(
	poolName: keyof typeof mainnetPools,
	side: 'ask' | 'bid' = 'ask',
) {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices', poolName, side],
		queryFn: async () => {
			const deepbookClient = await getDeepbookClient();
			return deepbookClient.getLevel2BookStatus(
				mainnetPools[poolName],
				BigInt(0),
				// TODO: need to switch back to 10n * DEFAULT_TICK_SIZE
				// 10n * DEFAULT_TICK_SIZE,
				10000n,
				side,
			);
		},
		enabled: !!poolName && !!mainnetPools[poolName],
	});
}

function getAveragePrice(coin: Coins, prices?: { price: bigint }[]) {
	if (!prices || !prices.length) {
		return 1;
	}

	const totalPrice = prices.reduce((acc: bigint, { price }: { price: bigint }) => {
		return acc + price;
	}, 0n);

	let divisor = OTHER_DIVISOR;
	if (coin === Coins.SUI) {
		divisor = SUI_DIVISOR;
	} else if (coin === Coins.USDT) {
		divisor = USDT_DIVISOR;
	}

	return new BigNumber(totalPrice.toString())
		.dividedBy(prices.length)
		.dividedBy(divisor)
		.toNumber();
}

function useAvgPrice(base: Coins, quote: Coins) {
	const { data: pricesSuiUsdc, ...restPricesSuiUsdc } = useDeepbookPrices('SUI_USDC_2');
	const { data: pricesWEthUsdc, ...restPricesWEthUsdc } = useDeepbookPrices('WETH_USDC_1');
	const { data: pricesTBtcUsdc, ...restPricesTBtcUsdc } = useDeepbookPrices('TBTC_USDC_1');
	const { data: pricesUsdtUsdc, ...restPricesUsdtUsdc } = useDeepbookPrices('USDT_USDC_1');

	const avgPriceSuiUsdc = useMemo(() => getAveragePrice(Coins.SUI, pricesSuiUsdc), [pricesSuiUsdc]);
	const avgPriceWEthUsdc = useMemo(
		() => getAveragePrice(Coins.WETH, pricesWEthUsdc),
		[pricesWEthUsdc],
	);
	const avgPriceTBtcUsdc = useMemo(
		() => getAveragePrice(Coins.tBTC, pricesTBtcUsdc),
		[pricesTBtcUsdc],
	);
	const avgPriceUsdtUsdc = useMemo(
		() => getAveragePrice(Coins.USDT, pricesUsdtUsdc),
		[pricesUsdtUsdc],
	);

	const refetchSuiWethUsdc = () =>
		Promise.all([restPricesSuiUsdc.refetch(), restPricesWEthUsdc.refetch()]);
	const refetchSuiTBtcUsdc = () =>
		Promise.all([restPricesSuiUsdc.refetch(), restPricesTBtcUsdc.refetch()]);
	const refetchSuiUsdtUsdc = () =>
		Promise.all([restPricesSuiUsdc.refetch(), restPricesUsdtUsdc.refetch()]);
	const refetchWethTBtcUsdc = () =>
		Promise.all([restPricesWEthUsdc.refetch(), restPricesTBtcUsdc.refetch()]);
	const refetchWethUsdtUsdc = () =>
		Promise.all([restPricesWEthUsdc.refetch(), restPricesUsdtUsdc.refetch()]);
	const refetchTbtcUsdtUsdc = () =>
		Promise.all([restPricesTBtcUsdc.refetch(), restPricesUsdtUsdc.refetch()]);

	const isRefetchingSuiWethUsdc = restPricesSuiUsdc.isRefetching || restPricesWEthUsdc.isRefetching;
	const isRefetchingSuiTBtcUsdc = restPricesSuiUsdc.isRefetching || restPricesTBtcUsdc.isRefetching;
	const isRefetchingSuiUsdtUsdc = restPricesSuiUsdc.isRefetching || restPricesUsdtUsdc.isRefetching;
	const isRefetchingWethTBtcUsdc =
		restPricesWEthUsdc.isRefetching || restPricesTBtcUsdc.isRefetching;
	const isRefetchingWethUsdtUsdc =
		restPricesWEthUsdc.isRefetching || restPricesUsdtUsdc.isRefetching;
	const isRefetchingTbtcUsdtUsdc =
		restPricesTBtcUsdc.isRefetching || restPricesUsdtUsdc.isRefetching;

	const defaultReturn = {
		averagePrice: 1,
		refetch: () => Promise.resolve(null),
		isRefetching: false,
	};

	if (quote === Coins.USDC) {
		if (base === Coins.SUI) {
			return {
				averagePrice: avgPriceSuiUsdc,
				refetch: restPricesSuiUsdc.refetch,
				isRefetching: restPricesSuiUsdc.isRefetching,
			};
		}
		if (base === Coins.WETH) {
			return {
				averagePrice: avgPriceWEthUsdc,
				refetch: restPricesWEthUsdc.refetch,
				isRefetching: restPricesWEthUsdc.isRefetching,
			};
		}
		if (base === Coins.tBTC) {
			return {
				averagePrice: avgPriceTBtcUsdc,
				refetch: restPricesTBtcUsdc.refetch,
				isRefetching: restPricesTBtcUsdc.isRefetching,
			};
		}
		if (base === Coins.USDT) {
			return {
				averagePrice: avgPriceUsdtUsdc,
				refetch: restPricesUsdtUsdc.refetch,
				isRefetching: restPricesUsdtUsdc.isRefetching,
			};
		}
		return defaultReturn;
	}

	if (base === Coins.SUI) {
		if (quote === Coins.WETH) {
			return {
				averagePrice: avgPriceSuiUsdc / avgPriceWEthUsdc,
				refetch: refetchSuiWethUsdc,
				isRefetching: isRefetchingSuiWethUsdc,
			};
		}
		if (quote === Coins.tBTC) {
			return {
				averagePrice: avgPriceSuiUsdc / avgPriceTBtcUsdc,
				refetch: refetchSuiTBtcUsdc,
				isRefetching: isRefetchingSuiTBtcUsdc,
			};
		}
		if (quote === Coins.USDT) {
			return {
				averagePrice: avgPriceSuiUsdc / avgPriceUsdtUsdc,
				refetch: refetchSuiUsdtUsdc,
				isRefetching: isRefetchingSuiUsdtUsdc,
			};
		}
		return defaultReturn;
	}

	if (base === Coins.WETH) {
		if (quote === Coins.SUI) {
			return {
				averagePrice: avgPriceWEthUsdc / avgPriceSuiUsdc,
				refetch: refetchSuiWethUsdc,
				isRefetching: isRefetchingSuiWethUsdc,
			};
		}
		if (quote === Coins.tBTC) {
			return {
				averagePrice: avgPriceWEthUsdc / avgPriceTBtcUsdc,
				refetch: refetchWethTBtcUsdc,
				isRefetching: isRefetchingWethTBtcUsdc,
			};
		}
		if (quote === Coins.USDT) {
			return {
				averagePrice: avgPriceWEthUsdc / avgPriceUsdtUsdc,
				refetch: refetchWethUsdtUsdc,
				isRefetching: isRefetchingWethUsdtUsdc,
			};
		}
		return defaultReturn;
	}

	if (base === Coins.tBTC) {
		if (quote === Coins.SUI) {
			return {
				averagePrice: avgPriceTBtcUsdc / avgPriceSuiUsdc,
				refetch: refetchSuiTBtcUsdc,
				isRefetching: isRefetchingSuiTBtcUsdc,
			};
		}
		if (quote === Coins.WETH) {
			return {
				averagePrice: avgPriceTBtcUsdc / avgPriceWEthUsdc,
				refetch: refetchWethTBtcUsdc,
				isRefetching: isRefetchingWethTBtcUsdc,
			};
		}
		if (quote === Coins.USDT) {
			return {
				averagePrice: avgPriceTBtcUsdc / avgPriceUsdtUsdc,
				refetch: refetchTbtcUsdtUsdc,
				isRefetching: isRefetchingWethUsdtUsdc,
			};
		}
		return defaultReturn;
	}

	if (base === Coins.USDT) {
		if (quote === Coins.SUI) {
			return {
				averagePrice: avgPriceUsdtUsdc / avgPriceSuiUsdc,
				refetch: refetchSuiUsdtUsdc,
				isRefetching: isRefetchingSuiUsdtUsdc,
			};
		}
		if (quote === Coins.WETH) {
			return {
				averagePrice: avgPriceUsdtUsdc / avgPriceWEthUsdc,
				refetch: refetchWethUsdtUsdc,
				isRefetching: isRefetchingWethUsdtUsdc,
			};
		}
		if (quote === Coins.tBTC) {
			return {
				averagePrice: avgPriceUsdtUsdc / avgPriceTBtcUsdc,
				refetch: refetchTbtcUsdtUsdc,
				isRefetching: isRefetchingTbtcUsdtUsdc,
			};
		}
		return defaultReturn;
	}

	return defaultReturn;
}

export function useBalanceConversion(
	balance: BigInt | BigNumber | null,
	base: Coins,
	quote: Coins,
) {
	const { averagePrice, ...rest } = useAvgPrice(base, quote);

	const rawValue = useMemo(() => {
		if (!averagePrice || !balance) return null;

		const walletBalanceInBase = new BigNumber(balance.toString()).toNumber();

		const rawUsdValue = walletBalanceInBase * averagePrice;

		if (isNaN(rawUsdValue)) {
			return null;
		}

		return rawUsdValue;
	}, [averagePrice, balance]);

	return {
		rawValue,
		averagePrice,
		...rest,
	};
}

export function useSuiBalanceInUSDC(suiBalance: BigInt | BigNumber | null) {
	return useBalanceConversion(suiBalance, Coins.SUI, Coins.USDC);
}
