// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getUSDCurrency, useSuiBalanceInUSDC } from '_app/hooks/useDeepBook';
import { Text } from '_app/shared/text';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { useCoinMetadata } from '@mysten/core';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useFormContext } from 'react-hook-form';
import { useSearchParams } from 'react-router-dom';

import { FEES_PERCENTAGE, type FormValues } from './utils';

export function GasFeeSection({ totalGas }: { totalGas?: string }) {
	const {
		formState: { isValid },
		watch,
	} = useFormContext<FormValues>();

	const [searchParams] = useSearchParams();

	const activeCoinType = searchParams.get('type');

	const { data: activeCoinData } = useCoinMetadata(activeCoinType);

	const amount = watch('amount');

	const estimatedFess = useMemo(() => {
		if (!amount || !isValid) {
			return null;
		}

		return new BigNumber(amount).times(FEES_PERCENTAGE);
	}, [amount, isValid]);

	const estimatedFessAsBigInt = estimatedFess ? new BigNumber(estimatedFess) : null;

	const { rawValue } = useSuiBalanceInUSDC(estimatedFessAsBigInt);

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
					{estimatedFess
						? `${estimatedFess.toLocaleString()} ${activeCoinData?.symbol} (${formattedEstimatedFees})`
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
					{totalGas ? parseFloat(totalGas).toLocaleString() : '--'}
				</Text>
			</DescriptionItem>
		</div>
	);
}
