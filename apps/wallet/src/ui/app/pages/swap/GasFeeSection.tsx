// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	Coins,
	getUSDCurrency,
	SUI_CONVERSION_RATE,
	useBalanceConversion,
} from '_app/hooks/useDeepBook';
import { Text } from '_app/shared/text';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { useCoinMetadata } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { FEES_PERCENTAGE } from './utils';

export function GasFeeSection({
	activeCoinType,
	totalGas,
	amount,
	isValid,
}: {
	activeCoinType: string | null;
	amount: string;
	isValid: boolean;
	totalGas: string;
}) {
	const { data: activeCoinData } = useCoinMetadata(activeCoinType);

	const estimatedFees = useMemo(() => {
		if (!amount || !isValid) {
			return null;
		}

		return new BigNumber(amount).times(FEES_PERCENTAGE);
	}, [amount, isValid]);

	const { rawValue } = useBalanceConversion(
		estimatedFees,
		activeCoinType === SUI_TYPE_ARG ? Coins.SUI : Coins.USDC,
		activeCoinType === SUI_TYPE_ARG ? Coins.USDC : Coins.SUI,
		activeCoinType === SUI_TYPE_ARG ? -SUI_CONVERSION_RATE : SUI_CONVERSION_RATE,
	);

	const formattedEstimatedFees = getUSDCurrency(rawValue);

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid">
			<DescriptionItem
				title={
					<Text variant="bodySmall" weight="medium" color="steel-dark">
						Fees ({FEES_PERCENTAGE}%)
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
					{totalGas && isValid ? parseFloat(totalGas).toLocaleString() : '--'}
				</Text>
			</DescriptionItem>
		</div>
	);
}
