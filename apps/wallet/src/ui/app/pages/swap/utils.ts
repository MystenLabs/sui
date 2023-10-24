// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { Coins, useBalanceConversion, useCoinsReFetchingConfig } from '_hooks';
import { SUI_CONVERSION_RATE } from '_pages/swap/constants';
import { useFormatCoin } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
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

	const { data: baseCoinBalanceData, isLoading: baseCoinBalanceDataLoading } = useSuiClientQuery(
		'getBalance',
		{ coinType: baseCoinType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const { data: quoteCoinBalanceData, isLoading: quoteCoinBalanceDataLoading } = useSuiClientQuery(
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
		isLoading: baseCoinBalanceDataLoading || quoteCoinBalanceDataLoading,
	};
}

export function useSuiUsdcBalanceConversion({ amount }: { amount: string }) {
	const suiUsdc = useBalanceConversion(
		new BigNumber(amount),
		Coins.SUI,
		Coins.USDC,
		-SUI_CONVERSION_RATE,
	);

	const usdcSui = useBalanceConversion(
		new BigNumber(amount),
		Coins.USDC,
		Coins.SUI,
		SUI_CONVERSION_RATE,
	);

	return {
		suiUsdc,
		usdcSui,
	};
}
