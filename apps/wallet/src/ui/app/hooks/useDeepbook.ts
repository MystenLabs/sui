// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getActiveNetworkSuiClient } from '_shared/sui-client';
import { DeepBookClient } from '@mysten/deepbook';
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
	TBTC = 'TBTC',
}

export const coinsMap = {
	[Coins.SUI]: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
	[Coins.USDC]: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
	[Coins.USDT]: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
	[Coins.WETH]: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
	[Coins.TBTC]: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
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

async function getDeepbookClient(): Promise<DeepBookClient> {
	const suiClient = await getActiveNetworkSuiClient();
	return new DeepBookClient(suiClient);
}

export function useDeepbookPools() {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: async () => {
			const deepbookClient = await getDeepbookClient();
			return deepbookClient.getAllPools({});
		},
	});
}

async function getPriceForPool(
	poolName: keyof typeof mainnetPools,
	deepbookClient: DeepBookClient,
) {
	const bid = deepbookClient.getLevel2BookStatus(
		mainnetPools[poolName],
		BigInt(0),
		// TODO: need to switch back to 10n * DEFAULT_TICK_SIZE
		// 10n * DEFAULT_TICK_SIZE,
		10000n,
		'bid',
	);

	const ask = deepbookClient.getLevel2BookStatus(
		mainnetPools[poolName],
		BigInt(0),
		// TODO: need to switch back to 10n * DEFAULT_TICK_SIZE
		// 10n * DEFAULT_TICK_SIZE,
		10000n,
		'ask',
	);

	return Promise.all([bid, ask]).then(([bid, ask]) => {
		const totalBidPrice = bid.reduce((acc, { price }) => {
			return acc + price;
		}, 0n);

		const averageBidPrice = bid.length ? totalBidPrice / BigInt(bid.length) : 0n;

		const totalAskPrice = ask.reduce((acc, { price }) => {
			return acc + price;
		}, 0n);

		const averageAskPrice = ask.length ? totalAskPrice / BigInt(ask.length) : 0n;

		if (averageBidPrice > 0n && averageAskPrice > 0n) {
			return (averageBidPrice + averageAskPrice) / 2n;
		}

		if (averageBidPrice > 0n) {
			return averageBidPrice;
		}

		return averageAskPrice;
	});
}

async function getDeepbookPriceForCoin(coin: Coins, deepbookClient: DeepBookClient) {
	const poolName1 = `${coin}_USDC_1` as keyof typeof mainnetPools;
	const poolName2 = coin === Coins.SUI ? 'SUI_USDC_2' : null;

	const promises = [getPriceForPool(poolName1, deepbookClient)];
	if (poolName2) {
		promises.push(getPriceForPool(poolName2, deepbookClient));
	}

	return Promise.all(promises).then(([price1, price2]) => {
		if (price1 && price2) {
			return (price1 + price2) / 2n;
		}

		return price1 || price2;
	});
}

function useDeepbookPricesInUSD(coins: Coins[]) {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', ...coins],
		queryFn: async () => {
			const deepbookClient = await getDeepbookClient();

			const promises = [];

			for (const coin of coins) {
				promises.push(getDeepbookPriceForCoin(coin, deepbookClient));
			}

			return Promise.all(promises);
		},
	});
}

function useAveragePrice(base: Coins, quote: Coins) {
	const { data, refetch, isRefetching } = useDeepbookPricesInUSD([
		Coins.SUI,
		Coins.WETH,
		Coins.TBTC,
		Coins.USDT,
	]);

	const priceSuiUsdc = new BigNumber((data?.[0] ?? 1n).toString());
	const priceWEthUsdc = new BigNumber((data?.[1] ?? 1n).toString());
	const priceTBtcUsdc = new BigNumber((data?.[2] ?? 1n).toString());
	const priceUsdtUsdc = new BigNumber((data?.[3] ?? 1n).toString());

	let averagePrice = new BigNumber(1n.toString());
	if (quote === Coins.USDC) {
		if (base === Coins.SUI) {
			averagePrice = priceSuiUsdc;
		} else if (base === Coins.WETH) {
			averagePrice = priceWEthUsdc;
		} else if (base === Coins.TBTC) {
			averagePrice = priceTBtcUsdc;
		} else if (base === Coins.USDT) {
			averagePrice = priceUsdtUsdc;
		}
	}

	if (base === Coins.SUI) {
		if (quote === Coins.WETH) {
			averagePrice = priceSuiUsdc.dividedBy(priceWEthUsdc);
		} else if (quote === Coins.TBTC) {
			averagePrice = priceSuiUsdc.dividedBy(priceTBtcUsdc);
		} else if (quote === Coins.USDT) {
			averagePrice = priceSuiUsdc.dividedBy(priceUsdtUsdc);
		}
	}

	if (base === Coins.WETH) {
		if (quote === Coins.SUI) {
			averagePrice = priceWEthUsdc.dividedBy(priceSuiUsdc);
		} else if (quote === Coins.TBTC) {
			averagePrice = priceWEthUsdc.dividedBy(priceTBtcUsdc);
		} else if (quote === Coins.USDT) {
			averagePrice = priceWEthUsdc.dividedBy(priceUsdtUsdc);
		}
	}

	if (base === Coins.TBTC) {
		if (quote === Coins.SUI) {
			averagePrice = priceTBtcUsdc.dividedBy(priceSuiUsdc);
		} else if (quote === Coins.WETH) {
			averagePrice = priceTBtcUsdc.dividedBy(priceWEthUsdc);
		} else if (quote === Coins.USDT) {
			averagePrice = priceTBtcUsdc.dividedBy(priceUsdtUsdc);
		}
	}

	if (base === Coins.USDT) {
		if (quote === Coins.SUI) {
			averagePrice = priceUsdtUsdc.dividedBy(priceSuiUsdc);
		} else if (quote === Coins.WETH) {
			averagePrice = priceUsdtUsdc.dividedBy(priceWEthUsdc);
		} else if (quote === Coins.TBTC) {
			averagePrice = priceUsdtUsdc.dividedBy(priceTBtcUsdc);
		}
	}

	return {
		averagePrice,
		refetch,
		isRefetching,
	};
}

export function useBalanceConversion(
	balance: BigInt | BigNumber | null,
	base: Coins,
	quote: Coins,
) {
	const { averagePrice, ...rest } = useAveragePrice(base, quote);

	const rawValue = useMemo(() => {
		if (!averagePrice || !balance) return null;

		const rawUsdValue = new BigNumber(balance.toString()).multipliedBy(averagePrice).toNumber();

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
