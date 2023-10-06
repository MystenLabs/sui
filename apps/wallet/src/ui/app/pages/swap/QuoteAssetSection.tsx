// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	Coins,
	getUSDCurrency,
	useBalanceConversion,
	useRecognizedCoins,
	useSuiBalanceInUSDC,
} from '_app/hooks/useDeepBook';
import { Text } from '_app/shared/text';
import { IconButton } from '_components/IconButton';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { AssetData } from '_pages/swap/AssetData';
import { MaxSlippage, MaxSlippageModal } from '_pages/swap/MaxSlippage';
import { QuoteAssets } from '_pages/swap/QuoteAssets';
import { useCoinMetadata } from '@mysten/core';
import { Refresh16 } from '@mysten/icons';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { useState } from 'react';
import { useFormContext } from 'react-hook-form';
import { useSearchParams } from 'react-router-dom';

import { useCoinTypeData, type FormValues } from './utils';

export function QuoteAssetSection() {
	const recognizedCoins = useRecognizedCoins();
	const [isQuoteAssetOpen, setQuoteAssetOpen] = useState(false);
	const [isSlippageModalOpen, setSlippageModalOpen] = useState(false);
	const {
		getValues,
		formState: { isValid },
	} = useFormContext<FormValues>();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');
	const { data: activeCoinData } = useCoinMetadata(activeCoinType);
	const quoteAssetType = getValues('quoteAssetType');
	const { formattedBalance: quoteAssetBalance, coinMetadata: quotedAssetMetaData } =
		useCoinTypeData(quoteAssetType);
	const quotedAssetSymbol = quotedAssetMetaData.data?.symbol ?? '';
	const amount = getValues('amount');

	const { rawValue, averagePrice, refetch, isRefetching } = useSuiBalanceInUSDC(
		new BigNumber(amount),
	);

	const averagePriceAsString = averagePrice?.toString();

	const { rawValue: rawValueQuoteToUsd } = useBalanceConversion(
		new BigNumber(rawValue || 0),
		quotedAssetSymbol as Coins,
		Coins.USDC,
	);

	if (!quotedAssetMetaData.data) {
		return null;
	}

	return (
		<div
			className={clsx(
				'flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid',
				isValid && 'bg-sui-primaryBlue2023/10',
			)}
		>
			<QuoteAssets
				recognizedCoins={recognizedCoins}
				isOpen={isQuoteAssetOpen}
				onClose={() => setQuoteAssetOpen(false)}
				onRowClick={(coinType) => {
					setQuoteAssetOpen(false);
				}}
			/>
			<AssetData
				disabled
				tokenBalance={quoteAssetBalance}
				coinType={quoteAssetType}
				symbol={quotedAssetSymbol}
				onClick={() => {
					setQuoteAssetOpen(true);
				}}
			/>
			<div
				className={clsx(
					'py-2 pr-2 pl-3 rounded-lg bg-gray-40 flex gap-2',
					isValid && 'border-solid border-hero-darkest/10',
				)}
			>
				{rawValue && !isRefetching ? (
					<>
						<Text variant="body" weight="semibold" color="steel-darker">
							{rawValue}
						</Text>
						<Text variant="body" weight="semibold" color="steel">
							{quotedAssetSymbol}
						</Text>
					</>
				) : (
					<Text variant="body" weight="semibold" color="steel">
						--
					</Text>
				)}
			</div>
			{rawValue && (
				<div className="ml-3">
					<DescriptionItem
						title={
							<Text variant="bodySmall" color="steel-dark">
								{isRefetching ? '--' : getUSDCurrency(rawValueQuoteToUsd)}
							</Text>
						}
					>
						<div className="flex gap-1 items-center">
							<Text variant="bodySmall" weight="medium" color="steel-dark">
								1 {activeCoinData?.symbol} = {isRefetching ? '--' : averagePriceAsString}{' '}
								{quotedAssetSymbol}
							</Text>
							<IconButton
								icon={<Refresh16 className="h-4 w-4 text-steel-dark hover:text-hero-dark" />}
								onClick={() => refetch()}
								loading={isRefetching}
							/>
						</div>
					</DescriptionItem>

					<div className="h-px w-full bg-hero-darkest/10 my-3" />

					<MaxSlippage setModalOpen={() => setSlippageModalOpen(true)} />
					<MaxSlippageModal
						isOpen={isSlippageModalOpen}
						onClose={() => setSlippageModalOpen(false)}
					/>
				</div>
			)}
		</div>
	);
}
