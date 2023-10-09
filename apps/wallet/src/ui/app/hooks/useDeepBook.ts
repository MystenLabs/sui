// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useSigner } from '_app/hooks/useSigner';
import { type WalletSigner } from '_app/WalletSigner';
import { useDeepBookClient } from '_shared/deepBook/context';
import { useGetObject, useGetOwnedObjects } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { type CoinStruct, type SuiClient } from '@mysten/sui.js/client';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 6;
export const USDC_DECIMALS = 9;
export const MAX_FLOAT = 2;

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

	return parseFloat(amount.toFixed(MAX_FLOAT)).toLocaleString('en', {
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

export function useCreateAccount({
	onSuccess,
	deepBookClient,
}: {
	onSuccess: () => void;
	deepBookClient: DeepBookClient;
}) {
	const activeAccount = useActiveAccount();
	const activeAccountAddress = activeAccount?.address;
	const signer = useSigner(activeAccount);

	return useMutation({
		mutationKey: [DEEPBOOK_KEY, 'create-account', activeAccountAddress],
		mutationFn: async () => {
			if (activeAccountAddress) {
				const txb = deepBookClient.createAccount(activeAccountAddress);

				return signer?.signAndExecuteTransactionBlock({ transactionBlock: txb });
			}

			return null;
		},
		onSuccess,
	});
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
	const bigIntBalance = BigInt(Math.floor(Number(balance)));

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

export async function getPlaceMarketOrderTxn({
	deepBookClient,
	poolId,
	balance,
	accountCapId,
	coins,
	coinType,
	address,
}: {
	deepBookClient: DeepBookClient;
	poolId: string;
	balance: string;
	accountCapId: string;
	coins: CoinStruct[];
	coinType: string;
	address: string;
}) {
	const txb = new TransactionBlock();

	let swapCoin;
	if (coinType === SUI_TYPE_ARG) {
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

	return await deepBookClient.placeMarketOrder(
		accountCap,
		poolId,
		BigInt(balance),
		coinType === SUI_TYPE_ARG ? 'ask' : 'bid',
		coinType === SUI_TYPE_ARG ? swapCoin : undefined,
		coinType === SUI_TYPE_ARG ? undefined : swapCoin,
		undefined,
		address,
		txb,
	);
}

export function useGetEstimate({
	balance,
	accountCapId,
	signer,
	coinType,
	poolId,
}: {
	balance: string;
	accountCapId: string;
	signer: WalletSigner | null;
	coinType: string;
	poolId: string;
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
			balance,
			coinType,
			activeAddress,
		],
		queryFn: async () => {
			const data = await getCoinsByBalance({
				coinType,
				balance,
				suiClient,
				address: activeAddress!,
			});

			if (!data?.length) {
				return null;
			}

			const txn = await getPlaceMarketOrderTxn({
				deepBookClient,
				poolId,
				balance,
				accountCapId,
				address: activeAddress!,
				coins: data,
				coinType,
			});

			if (!accountCapId) {
				queryClient.invalidateQueries(['get-owned-objects']);
			}

			return signer?.dryRunTransactionBlock({ transactionBlock: txn });
		},
		enabled: !!balance && !!signer && !!activeAddress,
	});
}
