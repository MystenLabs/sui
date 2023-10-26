// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { Coins, useBalanceConversion, useCoinsReFetchingConfig } from '_hooks';
import { SUI_CONVERSION_RATE } from '_pages/swap/constants';
import { roundFloat, useFormatCoin } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import BigNumber from 'bignumber.js';

export function useSwapData({
	baseCoinType,
	quoteCoinType,
	activeCoinType,
}: {
	baseCoinType: string;
	quoteCoinType: string;
	activeCoinType: string;
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

export function useSuiUsdcBalanceConversion({ amount }: { amount: string }) {
	const suiUsdc = useBalanceConversion({
		balance: new BigNumber(amount),
		from: Coins.SUI,
		to: Coins.USDC,
		conversionRate: -SUI_CONVERSION_RATE,
	});

	const usdcSui = useBalanceConversion({
		balance: new BigNumber(amount),
		from: Coins.USDC,
		to: Coins.SUI,
		conversionRate: SUI_CONVERSION_RATE,
	});

	return {
		suiUsdc,
		usdcSui,
	};
}

export function getUSDCurrency(amount?: number | null) {
	if (typeof amount !== 'number') {
		return null;
	}

	return roundFloat(amount).toLocaleString('en', {
		style: 'currency',
		currency: 'USD',
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
