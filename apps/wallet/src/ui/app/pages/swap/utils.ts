// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useCoinsReFetchingConfig } from '_hooks';
import { roundFloat, useFormatCoin } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import { type BalanceChange } from '@mysten/sui/client';
import BigNumber from 'bignumber.js';

export function useSwapData({
	baseCoinType,
	quoteCoinType,
}: {
	baseCoinType: string;
	quoteCoinType: string;
}) {
	const activeAccount = useActiveAccount();
	const activeAccountAddress = activeAccount?.address;
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: baseCoinBalanceData, isPending: baseCoinBalanceDataLoading } = useSuiClientQuery(
		'getBalance',
		{ coinType: baseCoinType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const { data: quoteCoinBalanceData, isPending: quoteCoinBalanceDataLoading } = useSuiClientQuery(
		'getBalance',
		{ coinType: quoteCoinType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const rawBaseBalance = baseCoinBalanceData?.totalBalance;
	const rawQuoteBalance = quoteCoinBalanceData?.totalBalance;

	const [formattedBaseBalance, baseCoinSymbol, baseCoinMetadata] = useFormatCoin(
		rawBaseBalance,
		baseCoinType,
	);
	const [formattedQuoteBalance, quoteCoinSymbol, quoteCoinMetadata] = useFormatCoin(
		rawQuoteBalance,
		quoteCoinType,
	);

	return {
		baseCoinBalanceData,
		quoteCoinBalanceData,
		formattedBaseBalance,
		formattedQuoteBalance,
		baseCoinSymbol,
		quoteCoinSymbol,
		baseCoinMetadata,
		quoteCoinMetadata,
		isPending: baseCoinBalanceDataLoading || quoteCoinBalanceDataLoading,
	};
}

export function getUSDCurrency(amount?: number | null) {
	if (typeof amount !== 'number') {
		return null;
	}

	return roundFloat(amount, 4).toLocaleString('en', {
		style: 'currency',
		currency: 'USD',
	});
}

export async function isExceedingSlippageTolerance({
	slipPercentage,
	poolId,
	deepBookClient,
	conversionRate,
	isAsk,
	average,
}: {
	slipPercentage: string;
	poolId: string;
	deepBookClient: DeepBookClient;
	conversionRate: number;
	isAsk: boolean;
	average: string;
}) {
	const convertedAverage = new BigNumber(average).shiftedBy(conversionRate).toString();

	const { bestBidPrice, bestAskPrice } = await deepBookClient.getMarketPrice(poolId);

	if (!bestBidPrice || !bestAskPrice) {
		return false;
	}

	const slip = new BigNumber(isAsk ? bestBidPrice.toString() : bestAskPrice.toString()).dividedBy(
		convertedAverage,
	);

	return new BigNumber('1').minus(slip).abs().isGreaterThan(slipPercentage);
}

function getCoinsFromBalanceChanges(coinType: string, balanceChanges: BalanceChange[]) {
	return balanceChanges
		.filter((balance) => {
			return balance.coinType === coinType;
		})
		.sort((a, b) => {
			const aAmount = new BigNumber(a.amount).abs();
			const bAmount = new BigNumber(b.amount).abs();

			return aAmount.isGreaterThan(bAmount) ? -1 : 1;
		});
}

export function getAverageFromBalanceChanges({
	balanceChanges,
	baseCoinType,
	quoteCoinType,
	isAsk,
	baseConversionRate,
	quoteConversionRate,
}: {
	balanceChanges: BalanceChange[];
	baseCoinType: string;
	quoteCoinType: string;
	isAsk: boolean;
	baseConversionRate: number;
	quoteConversionRate: number;
}) {
	const baseCoins = getCoinsFromBalanceChanges(baseCoinType, balanceChanges);
	const quoteCoins = getCoinsFromBalanceChanges(quoteCoinType, balanceChanges);

	if (!baseCoins.length || !quoteCoins.length) {
		return {
			averageBaseToQuote: '0',
			averageQuoteToBase: '0',
		};
	}

	const baseCoinAmount = new BigNumber(baseCoins[0].amount).abs();
	const quoteCoinAmount = new BigNumber(quoteCoins[0].amount).abs();
	const feesAmount = new BigNumber(isAsk ? baseCoins[1]?.amount : quoteCoins[1]?.amount)
		.shiftedBy(isAsk ? -baseConversionRate : -quoteConversionRate)
		.abs();

	const baseAndFees = baseCoinAmount.plus(feesAmount);
	const quoteAndFees = quoteCoinAmount.plus(feesAmount);

	const averageQuoteToBase = baseCoinAmount
		.dividedBy(isAsk ? quoteCoinAmount : quoteAndFees)
		.toString();
	const averageBaseToQuote = quoteCoinAmount
		.dividedBy(isAsk ? baseAndFees : baseCoinAmount)
		.toString();

	return {
		averageBaseToQuote,
		averageQuoteToBase,
	};
}

export function getBalanceConversion({
	balance,
	averages,
	isAsk,
}: {
	isAsk: boolean;
	balance: BigInt | BigNumber | null;
	averages: {
		averageBaseToQuote: string;
		averageQuoteToBase: string;
	};
}) {
	const bigNumberBalance = new BigNumber(balance?.toString() ?? '0');

	if (isAsk) {
		return bigNumberBalance.multipliedBy(averages.averageBaseToQuote).toString();
	}

	return bigNumberBalance.multipliedBy(averages.averageQuoteToBase).toString();
}
