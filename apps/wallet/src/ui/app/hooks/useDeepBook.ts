// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getActiveNetworkSuiClient } from '_shared/sui-client';
import { DeepBookClient } from '@mysten/deepbook';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const FLOAT_SCALING_FACTOR = 1_000_000_000n;
export const DEFAULT_TICK_SIZE = 1n * FLOAT_SCALING_FACTOR;
const DEEPBOOK_KEY = 'deepbook';

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

export const allowedSwapCoinsList = [SUI_TYPE_ARG, coinsMap[Coins.USDC]];

export function getUSDCurrency(amount: number | null) {
	if (typeof amount !== 'number') {
		return null;
	}

	return amount.toLocaleString('en', {
		style: 'currency',
		currency: 'USD',
	});
}

async function getDeepBookClient(): Promise<DeepBookClient> {
	const suiClient = await getActiveNetworkSuiClient();
	return new DeepBookClient(suiClient);
}

export function useDeepbookPools() {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: async () => {
			const deepBookClient = await getDeepBookClient();
			return deepBookClient.getAllPools({});
		},
	});
}

async function getPriceForPool(
	poolName: keyof typeof mainnetPools,
	deepBookClient: DeepBookClient,
) {
	const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(
		mainnetPools[poolName],
	);

	if (bestBidPrice && bestAskPrice) {
		return (bestBidPrice + bestAskPrice) / 2n;
	}

	return bestBidPrice || bestAskPrice;
}

async function getDeepBookPriceForCoin(coin: Coins, deepbookClient: DeepBookClient) {
	if (coin === Coins.USDC) {
		return 1n;
	}

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

async function getDeepbookPricesInUSD(coins: Coins[], deepBookClient: DeepBookClient) {
	const promises = coins.map((coin) => getDeepBookPriceForCoin(coin, deepBookClient));
	return Promise.all(promises);
}

function useDeepbookPricesInUSD(coins: Coins[]) {
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', ...coins],
		queryFn: async () => {
			const deepBookClient = await getDeepBookClient();

			return getDeepbookPricesInUSD(coins, deepBookClient);
		},
	});
}

function useAveragePrice(base: Coins, quote: Coins) {
	const { data: prices, ...rest } = useDeepbookPricesInUSD([base, quote]);

	const averagePrice = useMemo(() => {
		const basePrice = new BigNumber((prices?.[0] ?? 1n).toString());
		const quotePrice = new BigNumber((prices?.[1] ?? 1n).toString());

		const basePriceBigNumber = new BigNumber(basePrice.toString());
		const quotePriceBigNumber = new BigNumber(quotePrice.toString());

		let avgPrice;
		if (quote === Coins.USDC) {
			avgPrice = basePriceBigNumber;
		} else {
			avgPrice = basePriceBigNumber.dividedBy(quotePriceBigNumber);
		}

		return avgPrice;
	}, [prices, quote]);

	return {
		data: averagePrice,
		...rest,
	};
}

export function useBalanceConversion(
	balance: BigInt | BigNumber | null,
	base: Coins,
	quote: Coins,
) {
	const { data: averagePrice, ...rest } = useAveragePrice(base, quote);

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
