// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useCoinsReFetchingConfig } from '_hooks';
import { type SwapResult } from '_pages/swap/useSwapTransaction';
import { useFeatureValue } from '@growthbook/growthbook-react';
import {
	CoinFormat,
	formatBalance,
	getBalanceChangeSummary,
	getOwnerAddress,
	roundFloat,
	useCoinMetadata,
	useFormatCoin,
} from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { type TransactionEffects } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeStructTag, SUI_DECIMALS, SUI_TYPE_ARG } from '@mysten/sui/utils';
import BigNumber from 'bignumber.js';
import { useSearchParams } from 'react-router-dom';
import { z } from 'zod';

export const DEFAULT_MAX_SLIPPAGE_PERCENTAGE = 1;

export const W_USDC_TYPE_ARG =
	'0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN';
export const USDC_TYPE_ARG =
	'0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC';

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

export const maxSlippageFormSchema = z.object({
	allowedMaxSlippagePercentage: z
		.number({
			coerce: true,
			invalid_type_error: 'Input must be number only',
		})
		.positive()
		.max(100, 'Value must be between 0 and 100'),
});

export function useCoinTypesFromRouteParams() {
	const [searchParams] = useSearchParams();
	const fromCoinType = searchParams.get('type');
	const toCoinType = searchParams.get('toType');

	// Both are already defined, just use them:
	if (fromCoinType && toCoinType) {
		return { fromCoinType, toCoinType };
	}

	// Neither is set, default to SUI -> USDC
	if (!fromCoinType && !toCoinType) {
		return { fromCoinType: SUI_TYPE_ARG, toCoinType: USDC_TYPE_ARG };
	}

	return { fromCoinType, toCoinType };
}

export function useGetBalance({ coinType, owner }: { coinType?: string; owner?: string }) {
	const { data: coinMetadata } = useCoinMetadata(coinType);
	const refetchInterval = useFeatureValue('wallet-balance-refetch-interval', 20_000);

	return useSuiClientQuery(
		'getBalance',
		{
			coinType,
			owner: owner!,
		},
		{
			select: (data) => {
				const formatted = formatBalance(
					data.totalBalance,
					coinMetadata?.decimals ?? 0,
					CoinFormat.ROUNDED,
				);

				return {
					...data,
					formatted,
				};
			},
			refetchInterval,
			staleTime: 5_000,
			enabled: !!owner && !!coinType,
		},
	);
}

export const getTotalGasCost = (effects: TransactionEffects) => {
	return (
		BigInt(effects.gasUsed.computationCost) +
		BigInt(effects.gasUsed.storageCost) -
		BigInt(effects.gasUsed.storageRebate)
	);
};

export function formatSwapQuote({
	result,
	sender,
	fromType,
	toType,
	fromCoinDecimals,
	toCoinDecimals,
}: {
	fromCoinDecimals: number;
	fromType?: string;
	result: SwapResult;
	sender: string;
	toCoinDecimals: number;
	toType?: string;
}) {
	if (!result || !fromType || !toType) return null;

	const { dryRunResponse, fee } = result;
	const { balanceChanges } = dryRunResponse;
	const summary = getBalanceChangeSummary(dryRunResponse, []);
	const fromAmount =
		summary?.[sender]?.find(
			(bc) => normalizeStructTag(bc.coinType) === normalizeStructTag(fromType),
		)?.amount ?? 0n;
	const toAmount =
		summary?.[sender]?.find((bc) => normalizeStructTag(bc.coinType) === normalizeStructTag(toType))
			?.amount ?? 0n;

	const formattedToAmount = formatBalance(toAmount, toCoinDecimals);

	const estimatedRate = new BigNumber(toAmount.toString())
		.shiftedBy(fromCoinDecimals - toCoinDecimals)
		.dividedBy(new BigNumber(fromAmount.toString()).abs())
		.toFormat(toCoinDecimals);

	const accessFeeBalanceChange = balanceChanges.find(
		(bc) => ![fee.address, sender].includes(getOwnerAddress(bc.owner)),
	);

	const accessFees = new BigNumber((accessFeeBalanceChange?.amount || 0n).toString()).shiftedBy(
		-toCoinDecimals,
	);
	const coinOut = new BigNumber(toAmount.toString()).shiftedBy(-toCoinDecimals);
	const accessFeePercentage = accessFees.dividedBy(coinOut).multipliedBy(100).toFormat(3);

	const estimatedToAmount = new BigNumber(toAmount.toString())
		.shiftedBy(-toCoinDecimals)
		.minus(accessFees)
		.toFormat(toCoinDecimals);

	const gas = formatBalance(getTotalGasCost(dryRunResponse.effects), SUI_DECIMALS);

	return {
		provider: result?.provider,
		dryRunResponse,
		transaction: Transaction.from(result.bytes),
		estimatedRate,
		formattedToAmount,
		accessFeePercentage,
		accessFees: accessFees.toFormat(toCoinDecimals),
		accessFeeType: accessFeeBalanceChange?.coinType,
		estimatedToAmount,
		estimatedGas: gas,
		toAmount: toAmount.toString(),
		feePercentage: fee.percentage,
	};
}
