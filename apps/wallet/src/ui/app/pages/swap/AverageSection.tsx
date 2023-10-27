// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Text } from '_app/shared/text';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { MAX_FLOAT, SUI_USDC_AVERAGE_CONVERSION_RATE } from '_pages/swap/constants';
import { useSwapData } from '_pages/swap/utils';
import BigNumber from 'bignumber.js';

interface AverageSectionProps {
	averages: {
		averageBaseToQuote: string;
		averageQuoteToBase: string;
	};
	isAsk: boolean;
	baseCoinType: string;
	quoteCoinType: string;
}

export function AverageSection({
	averages,
	baseCoinType,
	quoteCoinType,
	isAsk,
}: AverageSectionProps) {
	const { baseCoinMetadata, quoteCoinMetadata } = useSwapData({
		baseCoinType,
		quoteCoinType,
	});

	const baseCoinSymbol = baseCoinMetadata.data?.symbol;
	const quoteCoinSymbol = quoteCoinMetadata.data?.symbol;

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid">
			<DescriptionItem title={`1 ${isAsk ? baseCoinSymbol : quoteCoinSymbol}`}>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{new BigNumber(isAsk ? averages.averageBaseToQuote : averages.averageQuoteToBase)
						.shiftedBy(isAsk ? SUI_USDC_AVERAGE_CONVERSION_RATE : -SUI_USDC_AVERAGE_CONVERSION_RATE)
						.decimalPlaces(MAX_FLOAT)
						.toString()}{' '}
					{isAsk ? quoteCoinSymbol : baseCoinSymbol}
				</Text>
			</DescriptionItem>
		</div>
	);
}
