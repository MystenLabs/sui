// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { type WalletSigner } from '_app/WalletSigner';
import {
	DEEPBOOK_KEY,
	DEFAULT_WALLET_FEE_ADDRESS,
	ESTIMATED_GAS_FEES_PERCENTAGE,
	ONE_SUI_DEEPBOOK,
	WALLET_FEES_PERCENTAGE,
} from '_pages/swap/constants';
import { useDeepBookContext } from '_shared/deepBook/context';
import { FEATURES } from '_shared/experimentation/features';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { roundFloat, useGetObject } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { type CoinStruct, type SuiClient } from '@mysten/sui.js/client';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

export enum Coins {
	SUI = 'SUI',
	USDC = 'USDC',
	USDT = 'USDT',
	WETH = 'WETH',
	TBTC = 'TBTC',
}

export const mainnetDeepBook: {
	pools: Record<string, string[]>;
	coinsMap: Record<Coins, string>;
} = {
	pools: {
		SUI_USDC: [
			'0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7',
			'0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be',
		],
		WETH_USDC: ['0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826'],
		TBTC_USDC: ['0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899'],
		USDT_USDC: ['0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955'],
	},
	coinsMap: {
		[Coins.SUI]: '0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
		[Coins.USDC]: '0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN',
		[Coins.USDT]: '0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN',
		[Coins.WETH]: '0xaf8cd5edc19c4512f4259f0bee101a40d41ebed738ade5874359610ef8eeced5::coin::COIN',
		[Coins.TBTC]: '0xbc3a676894871284b3ccfb2eec66f428612000e2a6e6d23f592ce8833c27c973::coin::COIN',
	},
};

export function useDeepBookConfigs() {
	return mainnetDeepBook;
}

export function useRecognizedCoins() {
	const coinsMap = useDeepBookConfigs().coinsMap;
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
	const deepBookClient = useDeepBookContext().client;

	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-all-pools'],
		queryFn: () => deepBookClient.getAllPools({}),
	});
}

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

function useDeepbookPricesInUSD(coins: Coins[], isAsk: boolean) {
	const deepBookClient = useDeepBookContext().client;
	const deepbookPools = useDeepBookConfigs().pools;

	return useQuery({
		queryKey: [DEEPBOOK_KEY, 'get-prices-usd', coins, isAsk],
		queryFn: async () => {
			const promises = coins.map((coin) =>
				getDeepBookPriceForCoin(coin, deepbookPools, isAsk, deepBookClient),
			);
			return Promise.all(promises);
		},
	});
}

function useAveragePrice(base: Coins, quote: Coins, isAsk: boolean) {
	const { data: prices, ...rest } = useDeepbookPricesInUSD([base, quote], isAsk);

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
	from: Coins,
	to: Coins,
	conversionRate: number = 1,
) {
	const { data: averagePrice, ...rest } = useAveragePrice(from, to, to === Coins.USDC);

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

	if (!coins.length) {
		throw new Error('No coins found in balance');
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
	return roundedDownBalance.abs().toString();
}

async function getPlaceMarketOrderTxn({
	deepBookClient,
	poolId,
	accountCapId,
	address,
	isAsk,
	lotSize,
	baseBalance,
	quoteBalance,
	quoteCoins,
	walletFeeAddress,
}: {
	deepBookClient: DeepBookClient;
	poolId: string;
	accountCapId: string;
	address: string;
	isAsk: boolean;
	lotSize: number;
	baseBalance: string;
	quoteBalance: string;
	baseCoins: CoinStruct[];
	quoteCoins: CoinStruct[];
	walletFeeAddress: string;
}) {
	const txb = new TransactionBlock();
	const accountCap = accountCapId || deepBookClient.createAccountCap(txb);

	let swapCoin;
	let balanceToSwap;
	let walletFeeCoin;
	let txnResult;

	if (isAsk) {
		const bigNumberBaseBalance = new BigNumber(baseBalance);

		if (bigNumberBaseBalance.isLessThan(ONE_SUI_DEEPBOOK)) {
			balanceToSwap = bigNumberBaseBalance.minus(
				bigNumberBaseBalance.times(ESTIMATED_GAS_FEES_PERCENTAGE / 100),
			);
		} else {
			balanceToSwap = bigNumberBaseBalance;
		}

		const walletFee = balanceToSwap
			.times(WALLET_FEES_PERCENTAGE / 100)
			.integerValue(BigNumber.ROUND_DOWN)
			.toString();

		balanceToSwap = formatBalanceToLotSize(balanceToSwap.minus(walletFee).toString(), lotSize);
		swapCoin = txb.splitCoins(txb.gas, [balanceToSwap]);
		walletFeeCoin = txb.splitCoins(txb.gas, [walletFee]);
		txnResult = await deepBookClient.placeMarketOrder(
			accountCap,
			poolId,
			BigInt(balanceToSwap),
			isAsk ? 'ask' : 'bid',
			isAsk ? swapCoin : undefined,
			isAsk ? undefined : swapCoin,
			undefined,
			address,
			txb,
		);
	} else {
		const primaryCoinInput = txb.object(quoteCoins[0].coinObjectId);
		const restCoins = quoteCoins.slice(1);

		if (restCoins.length) {
			txb.mergeCoins(
				primaryCoinInput,
				restCoins.map((coin) => txb.object(coin.coinObjectId)),
			);
		}

		const walletFee = new BigNumber(quoteBalance)
			.times(WALLET_FEES_PERCENTAGE / 100)
			.integerValue(BigNumber.ROUND_DOWN)
			.toString();

		balanceToSwap = new BigNumber(quoteBalance).minus(walletFee).toString();

		const [swapCoin, walletCoin] = txb.splitCoins(primaryCoinInput, [balanceToSwap, walletFee]);

		txnResult = await deepBookClient.swapExactQuoteForBase(
			poolId,
			swapCoin,
			BigInt(balanceToSwap),
			address,
			undefined,
			txb,
		);

		walletFeeCoin = walletCoin;
	}

	if (!accountCapId) {
		txnResult.transferObjects([accountCap], address);
	}

	if (walletFeeCoin) txnResult.transferObjects([walletFeeCoin], walletFeeAddress);

	return txnResult;
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
	const walletFeeAddress = useFeatureValue(FEATURES.WALLET_FEE_ADDRESS, DEFAULT_WALLET_FEE_ADDRESS);
	const queryClient = useQueryClient();
	const suiClient = useSuiClient();
	const activeAccount = useActiveAccount();
	const activeAddress = activeAccount?.address;
	const deepBookClient = useDeepBookContext().client;

	const { data } = useGetObject(poolId);
	const objectFields =
		data?.data?.content?.dataType === 'moveObject' ? data?.data?.content?.fields : null;

	const lotSize = (objectFields as Record<string, string>)?.lot_size;

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
			lotSize,
		],
		queryFn: async () => {
			const [baseCoins, quoteCoins] = await Promise.all([
				getCoinsByBalance({
					coinType,
					balance: baseBalance,
					suiClient,
					address: activeAddress!,
				}),
				getCoinsByBalance({
					coinType,
					balance: quoteBalance,
					suiClient,
					address: activeAddress!,
				}),
			]);

			if ((isAsk && !baseCoins.length) || (!isAsk && !quoteCoins.length)) {
				throw new Error('No coins found in balance');
			}

			const txn = await getPlaceMarketOrderTxn({
				deepBookClient,
				poolId,
				accountCapId,
				address: activeAddress!,
				isAsk,
				lotSize: Number(lotSize),
				baseCoins,
				quoteCoins,
				baseBalance,
				quoteBalance,
				walletFeeAddress,
			});

			if (!accountCapId) {
				await queryClient.invalidateQueries(['get-owned-objects']);
			}

			const dryRunResponse = await signer!.dryRunTransactionBlock({ transactionBlock: txn });

			return {
				txn,
				dryRunResponse,
			};
		},
		enabled:
			!!baseBalance &&
			baseBalance !== '0' &&
			!!quoteBalance &&
			quoteBalance !== '0' &&
			!!signer &&
			!!activeAddress,
	});
}

export async function isExceedingSlippageTolerance({
	slipPercentage,
	poolId,
	deepBookClient,
	conversionRate,
	baseCoinAmount,
	quoteCoinAmount,
	isAsk,
}: {
	slipPercentage: string;
	poolId: string;
	deepBookClient: DeepBookClient;
	conversionRate: number;
	baseCoinAmount?: string;
	quoteCoinAmount?: string;
	isAsk: boolean;
}) {
	if (!baseCoinAmount || !quoteCoinAmount) {
		return false;
	}

	const bigNumberBaseCoinAmount = new BigNumber(baseCoinAmount).abs();
	const bigNumberQuoteCoinAmount = new BigNumber(quoteCoinAmount).abs();

	const averagePricePaid = bigNumberQuoteCoinAmount
		.dividedBy(bigNumberBaseCoinAmount)
		.shiftedBy(conversionRate);

	const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(poolId);

	if (!bestBidPrice || !bestAskPrice) {
		return false;
	}

	const slip = new BigNumber(isAsk ? bestBidPrice.toString() : bestAskPrice.toString()).dividedBy(
		averagePricePaid,
	);

	return new BigNumber('1').minus(slip).abs().isGreaterThan(slipPercentage);
}
