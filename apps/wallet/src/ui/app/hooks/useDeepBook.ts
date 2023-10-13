// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { type WalletSigner } from '_app/WalletSigner';
import { useDeepBookClient } from '_shared/deepBook/context';
import { roundFloat } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { type CoinStruct, type SuiClient } from '@mysten/sui.js/client';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 6;
export const USDC_DECIMALS = 9;
export const MAX_FLOAT = 2;
const SUI_USDC_LOT_SIZE = 100000000;

export enum Coins {
	SUI = 'SUI',
	USDC = 'USDC',
	USDT = 'USDT',
	WETH = 'WETH',
	TBTC = 'TBTC',
}

export const mainnetDeepBook = {
	pools: {
		SUI_USDC_1: '0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be',
		SUI_USDC_2: '0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7',
		WETH_USDC_1: '0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826',
		TBTC_USDC_1: '0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899',
		USDT_USDC_1: '0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955',
	},
	coinsMap: {
		[Coins.SUI]: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
		[Coins.USDC]: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
		[Coins.USDT]: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
		[Coins.WETH]: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
		[Coins.TBTC]: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
	},
};

export function useMainnetPools() {
	return mainnetDeepBook.pools;
}

export function useMainnetCoinsMap() {
	return mainnetDeepBook.coinsMap;
}

export function useRecognizedCoins() {
	const coinsMap = useMainnetCoinsMap();
	return Object.values(coinsMap);
}

export const allowedSwapCoinsList = [SUI_TYPE_ARG, mainnetDeepBook.coinsMap[Coins.USDC]];

export function getUSDCurrency(amount: number | null) {
	if (typeof amount !== 'number') {
		return null;
	}

	return roundFloat(amount).toLocaleString('en', {
		style: 'currency',
		currency: 'USD',
	});
}

export function useDeepbookPools() {
	const deepBookClient = useDeepBookClient();

	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: () => deepBookClient.getAllPools({}),
	});
}

async function getPriceForPool(
	poolName: keyof (typeof mainnetDeepBook)['pools'],
	deepBookClient: DeepBookClient,
) {
	const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(
		mainnetDeepBook.pools[poolName],
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

	const poolName1 = `${coin}_USDC_1` as keyof (typeof mainnetDeepBook)['pools'];
	const poolName2 = coin === Coins.SUI ? 'SUI_USDC_2' : null;

	const promises = [getPriceForPool(poolName1, deepbookClient)];
	if (poolName2) {
		promises.push(getPriceForPool(poolName2, deepbookClient));
	}

	const [price1, price2] = await Promise.all(promises);

	if (price1 && price2) {
		return (price1 + price2) / 2n;
	}

	return price1 || price2;
}

function useDeepbookPricesInUSD(coins: Coins[]) {
	const deepBookClient = useDeepBookClient();
	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', coins],
		queryFn: async () => {
			const promises = coins.map((coin) => getDeepBookPriceForCoin(coin, deepBookClient));
			return Promise.all(promises);
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
	conversionRate: number = 1,
) {
	const { data: averagePrice, ...rest } = useAveragePrice(base, quote);
	const averagePriceWithConversion = averagePrice.shiftedBy(conversionRate);

	const rawValue = useMemo(() => {
		if (!averagePriceWithConversion || !balance) return null;

		const rawUsdValue = new BigNumber(balance.toString())
			.multipliedBy(averagePriceWithConversion)
			.toNumber();

		if (isNaN(rawUsdValue)) {
			return null;
		}

		return rawUsdValue;
	}, [averagePriceWithConversion, balance]);

	return {
		rawValue,
		averagePrice: averagePriceWithConversion,
		...rest,
	};
}

const MAX_COINS_PER_REQUEST = 10;

export async function getCoinsByBalance({
	coinType,
	balance,
	suiClient,
	address,
}: {
	coinType: string;
	balance: string;
	suiClient: SuiClient;
	address: string;
}) {
	let cursor: string | undefined | null = null;
	let currentBalance = 0n;
	let hasNextPage = true;
	const coins = [];
	const bigIntBalance = BigInt(new BigNumber(balance).integerValue(BigNumber.ROUND_UP).toString());

	while (currentBalance < bigIntBalance && hasNextPage) {
		const { data, nextCursor } = await suiClient.getCoins({
			owner: address,
			coinType,
			cursor,
			limit: MAX_COINS_PER_REQUEST,
		});

		if (!data || !data.length) {
			break;
		}

		for (const coin of data) {
			currentBalance += BigInt(coin.balance);
			coins.push(coin);

			if (currentBalance >= bigIntBalance) {
				break;
			}
		}

		cursor = nextCursor;
		hasNextPage = !!nextCursor;
	}

	return coins;
}

function formatBalanceToLotSize(balance: string, lotSize: number) {
	const balanceBigNumber = new BigNumber(balance);
	const lotSizeBigNumber = new BigNumber(lotSize);
	const remainder = balanceBigNumber.mod(lotSizeBigNumber);

	if (remainder.isEqualTo(0)) {
		return balanceBigNumber.toString();
	}

	const roundedDownBalance = balanceBigNumber.minus(remainder);
	return roundedDownBalance.toString();
}

export async function getPlaceMarketOrderTxn({
	deepBookClient,
	poolId,
	balance,
	accountCapId,
	coins,
	address,
	isAsk,
}: {
	deepBookClient: DeepBookClient;
	poolId: string;
	balance: string;
	accountCapId: string;
	coins: CoinStruct[];
	address: string;
	isAsk: boolean;
}) {
	const txb = new TransactionBlock();

	let swapCoin;
	if (isAsk) {
		swapCoin = txb.splitCoins(txb.gas, [balance]);
	} else {
		const primaryCoinInput = txb.object(coins[0].coinObjectId);
		const restCoins = coins.slice(1);

		if (restCoins.length) {
			txb.mergeCoins(
				primaryCoinInput,
				coins.slice(1).map((coin) => txb.object(coin.coinObjectId)),
			);
		}

		const balance = coins.reduce((acc, coin) => acc + BigInt(coin.balance), 0n);
		swapCoin = txb.splitCoins(primaryCoinInput, [balance]);
	}

	const accountCap = accountCapId || deepBookClient.createAccountCap(txb);

	const validBalance = formatBalanceToLotSize(balance, SUI_USDC_LOT_SIZE);

	return await deepBookClient.placeMarketOrder(
		accountCap,
		poolId,
		BigInt(validBalance),
		isAsk ? 'ask' : 'bid',
		isAsk ? swapCoin : undefined,
		isAsk ? undefined : swapCoin,
		undefined,
		address,
		txb,
	);
}

export function useGetEstimate({
	accountCapId,
	signer,
	coinType,
	poolId,
	baseBalance,
	quoteBalance,
	isAsk,
}: {
	accountCapId: string;
	signer: WalletSigner | null;
	coinType: string;
	poolId: string;
	baseBalance: string;
	quoteBalance: string;
	isAsk: boolean;
}) {
	const queryClient = useQueryClient();
	const suiClient = useSuiClient();
	const activeAccount = useActiveAccount();
	const activeAddress = activeAccount?.address;
	const deepBookClient = useDeepBookClient();

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: [
			DEEPBOOK_KEY,
			'get-estimate',
			poolId,
			accountCapId,
			coinType,
			activeAddress,
			baseBalance,
			quoteBalance,
			isAsk,
		],
		queryFn: async () => {
			const coins = await getCoinsByBalance({
				coinType,
				balance: isAsk ? baseBalance : quoteBalance,
				suiClient,
				address: activeAddress!,
			});

			if (!coins?.length) {
				return null;
			}

			const txn = await getPlaceMarketOrderTxn({
				deepBookClient,
				poolId,
				balance: baseBalance,
				accountCapId,
				address: activeAddress!,
				coins,
				isAsk,
			});

			if (!accountCapId) {
				await queryClient.invalidateQueries(['get-owned-objects']);
			}

			const dryRunResponse = await signer?.dryRunTransactionBlock({ transactionBlock: txn });

			return {
				txn,
				dryRunResponse,
			};
		},
		enabled: !!baseBalance && !!quoteBalance && !!signer && !!activeAddress,
	});
}

export async function isExceedingSlippageTolerance({
	slipPercentage,
	averagePrice,
	poolId,
	deepBookClient,
	isAsk,
}: {
	slipPercentage: string;
	averagePrice: BigNumber;
	poolId: string;
	deepBookClient: DeepBookClient;
	isAsk: boolean;
}) {
	const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(poolId);
	const slipPercentageDecimal = parseFloat(slipPercentage) / 100;
	const slippageTolerance = averagePrice.multipliedBy(slipPercentageDecimal);
	const priceDifference = averagePrice
		.minus(((isAsk ? bestAskPrice : bestBidPrice) || 0n).toString())
		.absoluteValue();

	return priceDifference.isGreaterThan(slippageTolerance);
}
