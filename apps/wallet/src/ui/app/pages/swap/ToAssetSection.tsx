// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import {
	Coins,
	getUSDCurrency,
	MAX_FLOAT,
	SUI_CONVERSION_RATE,
	USDC_DECIMALS,
	useBalanceConversion,
	useMainnetCoinsMap,
	useRecognizedCoins,
} from '_app/hooks/useDeepBook';
import { Text } from '_app/shared/text';
import { IconButton } from '_components/IconButton';
import { useCoinsReFetchingConfig } from '_hooks';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { AssetData } from '_pages/swap/AssetData';
import { MaxSlippage, MaxSlippageModal } from '_pages/swap/MaxSlippage';
import { QuoteAssets } from '_pages/swap/QuoteAssets';
import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { Refresh16 } from '@mysten/icons';
import { type BalanceChange } from '@mysten/sui.js/client';
import { SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { useEffect, useState } from 'react';
import { useFormContext } from 'react-hook-form';

import { type FormValues } from './utils';

export function ToAssetSection({
	activeCoinType,
	balanceChanges,
}: {
	activeCoinType: string | null;
	balanceChanges: BalanceChange[];
}) {
	const coinsMap = useMainnetCoinsMap();
	const activeAccount = useActiveAccount();
	const activeAccountAddress = activeAccount?.address;
	const recognizedCoins = useRecognizedCoins();
	const [isQuoteAssetOpen, setQuoteAssetOpen] = useState(false);
	const [isSlippageModalOpen, setSlippageModalOpen] = useState(false);

	const {
		watch,
		setValue,
		formState: { isValid },
	} = useFormContext<FormValues>();
	const { data: activeCoinData } = useCoinMetadata(activeCoinType);
	const quoteAssetType = watch('quoteAssetType');
	const rawQuoteAssetAmount = balanceChanges.find(
		(balanceChange) => balanceChange.coinType === quoteAssetType,
	)?.amount;

	const quotedAssetAmount = new BigNumber(rawQuoteAssetAmount || '0').shiftedBy(
		activeCoinType === SUI_TYPE_ARG ? -SUI_CONVERSION_RATE : -USDC_DECIMALS,
	);

	const quotedAssetAmountAsNum = quotedAssetAmount.toNumber();

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	useEffect(() => {
		const newQuoteAsset = activeCoinType === SUI_TYPE_ARG ? coinsMap[Coins.USDC] : SUI_TYPE_ARG;
		setValue('quoteAssetType', newQuoteAsset);
	}, [activeCoinType, coinsMap, quoteAssetType, setValue]);

	const { data: coinBalanceData } = useSuiClientQuery(
		'getBalance',
		{ coinType: quoteAssetType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const coinBalance = coinBalanceData?.totalBalance;

	const [quoteAssetBalance, _, quotedAssetMetaData] = useFormatCoin(coinBalance, quoteAssetType);

	const quotedAssetSymbol = quotedAssetMetaData.data?.symbol ?? '';
	const amount = watch('amount');

	const { rawValue, averagePrice, refetch, isRefetching } = useBalanceConversion(
		new BigNumber(amount),
		activeCoinType === SUI_TYPE_ARG ? Coins.SUI : Coins.USDC,
		activeCoinType === SUI_TYPE_ARG ? Coins.USDC : Coins.SUI,
		activeCoinType === SUI_TYPE_ARG ? -SUI_CONVERSION_RATE : SUI_CONVERSION_RATE,
	);

	const averagePriceAsString = averagePrice.toFixed(MAX_FLOAT).toString();

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
				{quotedAssetAmountAsNum && !isRefetching ? (
					<>
						<Text variant="body" weight="semibold" color="steel-darker">
							{quotedAssetAmountAsNum}
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
								{isRefetching
									? '--'
									: getUSDCurrency(
											activeCoinType === SUI_TYPE_ARG ? quotedAssetAmountAsNum : Number(amount),
									  )}
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

					<MaxSlippage onOpen={() => setSlippageModalOpen(true)} />
					<MaxSlippageModal
						isOpen={isSlippageModalOpen}
						onClose={() => setSlippageModalOpen(false)}
					/>
				</div>
			)}
		</div>
	);
}
