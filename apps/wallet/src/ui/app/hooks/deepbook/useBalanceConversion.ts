// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Coins, useDeepBookConfigs } from '_app/hooks';
import { DEEPBOOK_KEY } from '_pages/swap/constants';
import { useDeepBookContext } from '_shared/deepBook/context';
import { type DeepBookClient } from '@mysten/deepbook';
import { useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';

async function getDeepBookPriceForCoin(
	coin: Coins,
	pools: Record<string, string[]>,
	isAsk: boolean,
	deepBookClient: DeepBookClient,
) {
	if (coin === Coins.USDC) {
		return 1n;
	}

	const poolName = `${coin}_USDC`;
	const poolIds = pools[poolName];
	const promises = poolIds.map(async (poolId) => {
		const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(poolId);

		return isAsk ? bestBidPrice : bestAskPrice;
	});

	const prices = await Promise.all(promises);

	const filter: bigint[] = prices.filter((price): price is bigint => {
		return typeof price === 'bigint' && price !== 0n;
	});

	const total = filter.reduce((acc, price) => {
		return acc + price;
	}, 0n);

	return total / BigInt(filter.length);
}

export function useBalanceConversion({
	balance,
	from,
	to,
	conversionRate = 1,
}: {
	balance: BigInt | BigNumber | null;
	from: Coins;
	to: Coins;
	conversionRate: number;
}) {
	const deepBookClient = useDeepBookContext().client;
	const deepbookPools = useDeepBookConfigs().pools;
	const isAsk = to === Coins.USDC;

	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', isAsk, from, to],
		queryFn: async () => {
			const coins = [from, to];
			const promises = coins.map((coin) =>
				getDeepBookPriceForCoin(coin, deepbookPools, isAsk, deepBookClient),
			);
			return Promise.all(promises);
		},
		select: (prices) => {
			const basePrice = new BigNumber((prices?.[0] ?? 1n).toString());
			const quotePrice = new BigNumber((prices?.[1] ?? 1n).toString());

			const basePriceBigNumber = new BigNumber(basePrice.toString());
			const quotePriceBigNumber = new BigNumber(quotePrice.toString());

			let avgPrice;
			if (to === Coins.USDC) {
				avgPrice = basePriceBigNumber;
			} else {
				avgPrice = basePriceBigNumber.dividedBy(quotePriceBigNumber);
			}

			const averagePriceWithConversion = avgPrice?.shiftedBy(conversionRate);

			if (!averagePriceWithConversion || !balance) return null;

			const rawUsdValue = new BigNumber(balance.toString())
				.multipliedBy(averagePriceWithConversion)
				.toNumber();

			if (isNaN(rawUsdValue)) {
				return null;
			}

			return {
				rawValue: rawUsdValue,
				averagePrice: averagePriceWithConversion,
			};
		},
	});
}
