// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRecognizedCoins } from '_app/hooks/deepbook';
import { Button } from '_app/shared/ButtonUI';
import { InputWithActionButton } from '_app/shared/InputWithAction';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import { AssetData } from '_pages/swap/AssetData';
import {
	Coins,
	SUI_CONVERSION_RATE,
	USDC_CONVERSION_RATE,
	type FormValues,
} from '_pages/swap/constants';
import { MaxSlippage, MaxSlippageModal } from '_pages/swap/MaxSlippage';
import { ToAssets } from '_pages/swap/ToAssets';
import { getUSDCurrency, useSwapData } from '_pages/swap/utils';
import { useDeepBookContext } from '_shared/deepBook/context';
import { type BalanceChange } from '@mysten/sui/client';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import BigNumber from 'bignumber.js';
import clsx from 'clsx';
import { useEffect, useState } from 'react';
import { useFormContext } from 'react-hook-form';

export function ToAssetSection({
	activeCoinType,
	balanceChanges,
	slippageErrorString,
	baseCoinType,
	quoteCoinType,
	loading,
	refetch,
	error,
}: {
	activeCoinType: string | null;
	balanceChanges: BalanceChange[];
	slippageErrorString: string;
	baseCoinType: string;
	quoteCoinType: string;
	loading: boolean;
	refetch: () => void;
	error: Error | null;
}) {
	const coinsMap = useDeepBookContext().configs.coinsMap;
	const recognizedCoins = useRecognizedCoins();
	const [isToAssetOpen, setToAssetOpen] = useState(false);
	const [isSlippageModalOpen, setSlippageModalOpen] = useState(false);
	const isAsk = activeCoinType === SUI_TYPE_ARG;

	const { formattedBaseBalance, formattedQuoteBalance, baseCoinMetadata, quoteCoinMetadata } =
		useSwapData({
			baseCoinType,
			quoteCoinType,
		});

	const toAssetBalance = isAsk ? formattedQuoteBalance : formattedBaseBalance;
	const toAssetMetaData = isAsk ? quoteCoinMetadata : baseCoinMetadata;

	const {
		watch,
		setValue,
		formState: { isValid },
	} = useFormContext<FormValues>();
	const toAssetType = watch('toAssetType');

	const rawToAssetAmount = balanceChanges.find(
		(balanceChange) => balanceChange.coinType === toAssetType,
	)?.amount;

	const toAssetAmountAsNum = new BigNumber(rawToAssetAmount || '0')
		.shiftedBy(isAsk ? -SUI_CONVERSION_RATE : -USDC_CONVERSION_RATE)
		.toNumber();

	useEffect(() => {
		const newToAsset = isAsk ? coinsMap[Coins.USDC] : SUI_TYPE_ARG;
		setValue('toAssetType', newToAsset);
	}, [coinsMap, isAsk, setValue]);

	const toAssetSymbol = toAssetMetaData.data?.symbol ?? '';
	const amount = watch('amount');

	if (!toAssetMetaData.data) {
		return null;
	}

	return (
		<div
			className={clsx(
				'flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid',
				{ 'bg-sui-primaryBlue2023/10': isValid },
			)}
		>
			<ToAssets
				recognizedCoins={recognizedCoins}
				isOpen={isToAssetOpen}
				onClose={() => setToAssetOpen(false)}
				onRowClick={(coinType) => {
					setToAssetOpen(false);
				}}
			/>
			<AssetData
				disabled
				tokenBalance={toAssetBalance}
				coinType={toAssetType}
				symbol={toAssetSymbol}
				onClick={() => {
					setToAssetOpen(true);
				}}
			/>

			<InputWithActionButton
				name="output-amount"
				disabled
				noBorder={!isValid}
				placeholder="--"
				value={toAssetAmountAsNum || '--'}
				loading={loading}
				loadingText="Calculating..."
				suffix={
					!!toAssetAmountAsNum &&
					!loading && (
						<Text variant="body" weight="semibold" color="steel">
							{toAssetSymbol}
						</Text>
					)
				}
				info={
					isValid && (
						<Text variant="subtitleSmall" color="steel-dark">
							{getUSDCurrency(isAsk ? toAssetAmountAsNum : Number(amount))}
						</Text>
					)
				}
			/>

			{isValid && toAssetAmountAsNum && amount ? (
				<div className="ml-3">
					<MaxSlippage onOpen={() => setSlippageModalOpen(true)} />

					{slippageErrorString && (
						<div className="mt-2">
							<Alert>{slippageErrorString}</Alert>
						</div>
					)}

					<MaxSlippageModal
						isOpen={isSlippageModalOpen}
						onClose={() => setSlippageModalOpen(false)}
					/>
				</div>
			) : null}

			{error && (
				<div className="flex flex-col gap-4">
					<Alert>
						<Text variant="pBody" weight="semibold">
							Calculation failed
						</Text>
						<Text variant="pBodySmall">{error.message || 'An error has occurred, try again.'}</Text>
					</Alert>
					<Button text="Recalculate" onClick={refetch} />
				</div>
			)}
		</div>
	);
}
