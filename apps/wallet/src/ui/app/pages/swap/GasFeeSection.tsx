// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Text } from '_app/shared/text';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { SUI_USDC_AVERAGE_CONVERSION_RATE, WALLET_FEES_PERCENTAGE } from '_pages/swap/constants';
import { getBalanceConversion, getUSDCurrency } from '_pages/swap/utils';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

export function GasFeeSection({
	activeCoinType,
	totalGas,
	amount,
	isValid,
	averages,
}: {
	activeCoinType: string | null;
	amount: string;
	isValid: boolean;
	totalGas: string;
	averages: {
		averageBaseToQuote: string;
		averageQuoteToBase: string;
	};
}) {
	const { data: activeCoinData } = useCoinMetadata(activeCoinType);
	const isAsk = activeCoinType === SUI_TYPE_ARG;

	const estimatedFees = useMemo(() => {
		if (!amount || !isValid) {
			return null;
		}

		return new BigNumber(amount).times(WALLET_FEES_PERCENTAGE / 100);
	}, [amount, isValid]);

	const rawValue = getBalanceConversion({
		balance: estimatedFees,
		isAsk,
		averages,
	});

	const convertedRawValue = new BigNumber(rawValue)
		.shiftedBy(isAsk ? SUI_USDC_AVERAGE_CONVERSION_RATE : -SUI_USDC_AVERAGE_CONVERSION_RATE)
		.toNumber();

	const [gas, symbol] = useFormatCoin(totalGas, GAS_TYPE_ARG);

	const formattedEstimatedFees = getUSDCurrency(convertedRawValue);

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl px-5 py-3 gap-2 border-solid">
			<DescriptionItem
				title={
					<Text variant="bodySmall" weight="medium" color="steel-dark">
						Access Fees ({WALLET_FEES_PERCENTAGE}%)
					</Text>
				}
			>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{estimatedFees
						? `${estimatedFees.toLocaleString()} ${activeCoinData?.symbol} (${formattedEstimatedFees})`
						: '--'}
				</Text>
			</DescriptionItem>

			<div className="bg-gray-40 h-px w-full" />

			<DescriptionItem
				title={
					<Text variant="bodySmall" weight="medium" color="steel-dark">
						Estimated Gas Fee
					</Text>
				}
			>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{totalGas && isValid ? `${gas} ${symbol}` : '--'}
				</Text>
			</DescriptionItem>
		</div>
	);
}
