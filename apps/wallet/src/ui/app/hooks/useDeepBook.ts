// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useSigner } from '_app/hooks/useSigner';
import { type WalletSigner } from '_app/WalletSigner';
import { useDeepBookClient } from '_shared/deepBook/context';
import { useGetObject, useGetOwnedObjects } from '@mysten/core';
import { type DeepBookClient } from '@mysten/deepbook';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useMutation, useQuery } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 1e6;

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

	return amount.toLocaleString('en', {
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

async function getDeepbookPricesInUSD(coins: Coins[], deepBookClient: DeepBookClient) {
	const promises = coins.map((coin) => getDeepBookPriceForCoin(coin, deepBookClient));
	return Promise.all(promises);
}

function useDeepbookPricesInUSD(coins: Coins[]) {
	const deepBookClient = useDeepBookClient();
	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', ...coins],
		queryFn: async () => {
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
	conversionRate: number = 1,
) {
	const { data: averagePrice, ...rest } = useAveragePrice(base, quote);
	const averagePriceWithConversion = averagePrice.dividedBy(conversionRate);

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

export function useSuiBalanceInUSDC(suiBalance: BigInt | BigNumber | null) {
	return useBalanceConversion(suiBalance, Coins.SUI, Coins.USDC, SUI_CONVERSION_RATE);
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

export function useMarketAccountCap(activeAccountAddress?: string) {
	const {
		data,
		isLoading: getOwnedObjectsLoading,
		isError: getOwnedObjectsError,
		isRefetching: isRefetchingOwnedObjects,
		refetch,
	} = useGetOwnedObjects(
		activeAccountAddress,
		{
			MatchAll: [{ StructType: '0xdee9::custodian_v2::AccountCap' }],
		},
		10,
	);

	const objectId = data?.pages?.[0]?.data?.[0]?.data?.objectId;

	const {
		data: suiObjectResponseData,
		isLoading: getObjectLoading,
		isRefetching: isRefetchingObject,
		isError: getObjectError,
	} = useGetObject(objectId);

	const fieldsData =
		suiObjectResponseData?.data?.content?.dataType === 'moveObject'
			? (suiObjectResponseData?.data?.content?.fields as Record<string, string | number | object>)
			: null;

	return {
		data: fieldsData,
		isLoading: getObjectLoading || getOwnedObjectsLoading,
		isError: getOwnedObjectsError || getObjectError,
		isRefetching: isRefetchingObject || isRefetchingOwnedObjects,
		refetch,
	};
}

export function useGetEstimateSuiToUSDC({
	balanceInMist,
	accountCapId,
	signer,
}: {
	balanceInMist: string;
	accountCapId: string;
	signer: WalletSigner | null;
}) {
	const mainnetPools = useMainnetPools();
	const deepBookClient = useDeepBookClient();
	const poolId = mainnetPools.SUI_USDC_2;

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: [DEEPBOOK_KEY, 'get-estimate', poolId, accountCapId],
		queryFn: async () => {
			const txb = new TransactionBlock();

			// if SUI > USDC
			const baseCoin = txb.splitCoins(txb.gas, [balanceInMist]);

			// if USDC > SUI
			// useGetAllCoins implementation. Paginate through until get desired balance
			// value in coin metaData

			const txn = await deepBookClient.placeMarketOrder(
				mainnetPools.SUI_USDC_2,
				1000000000n,
				'ask',
				baseCoin,
				undefined,
				undefined,
				accountCapId,
				txb,
			);

			if (signer && txn) {
				return signer.dryRunTransactionBlock({ transactionBlock: txn });
			}

			return null;
		},
		enabled: !!balanceInMist && !!signer && !!accountCapId,
	});
}
