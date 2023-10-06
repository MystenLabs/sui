// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useSigner } from '_app/hooks/useSigner';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Form } from '_app/shared/forms/Form';
import { InputWithActionButton } from '_app/shared/InputWithAction';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import {
	allowedSwapCoinsList,
	getUSDCurrency,
	useCoinsReFetchingConfig,
	useCreateAccount,
	useGetEstimateSuiToUSDC,
	useMainnetCoinsMap,
	useMarketAccountCap,
	useSortedCoinsByCategories,
	useSuiBalanceInUSDC,
} from '_hooks';
import { useTransactionSummary, useZodForm } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16 } from '@mysten/icons';
import { type DryRunTransactionBlockResponse } from '@mysten/sui.js/client';
import { MIST_PER_SUI } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { useEffect, useMemo } from 'react';
import { useWatch, type SubmitHandler } from 'react-hook-form';
import { Route, Routes, useNavigate, useSearchParams } from 'react-router-dom';
import { z } from 'zod';

import { AssetData } from './AssetData';
import { BaseAssets } from './BaseAssets';
import { GasFeeSection } from './GasFeeSection';
import { QuoteAssetSection } from './QuoteAssetSection';
import { initialValues, useCoinTypeData, type FormValues } from './utils';

function getSwapPageAtcText(
	fromSymbol: string,
	quoteAssetType: string,
	coinsMap: Record<string, string>,
) {
	const toSymbol =
		Object.entries(coinsMap).find(([key, value]) => value === quoteAssetType)?.[0] || '';

	return `Swap ${fromSymbol} to ${toSymbol}`;
}

function SwapPageForm() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const activeAccount = useActiveAccount();
	const signer = useSigner(activeAccount);
	const activeAccountAddress = activeAccount?.address;
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();
	const { mutate } = useCreateAccount();
	const {
		data: marketAccountCapData,
		refetch: refetchUseMarketAccountCapLoading,
		isLoading: useMarketAccountCapLoading,
	} = useMarketAccountCap(activeAccount?.address);
	const coinsMap = useMainnetCoinsMap();

	const accountCapId = marketAccountCapData?.owner as string;

	const activeCoinType = searchParams.get('type');

	const { isLoading, formattedBalance, coinMetadata } = useCoinTypeData(activeCoinType);

	const { data: coinBalances } = useSuiClientQuery(
		'getAllBalances',
		{ owner: activeAccountAddress! },
		{
			enabled: !!activeAccountAddress,
			staleTime,
			refetchInterval,
			select: filterAndSortTokenBalances,
		},
	);

	const { recognized } = useSortedCoinsByCategories(coinBalances ?? []);

	const formattedTokenBalance = formattedBalance.replace(/,/g, '');
	const symbol = coinMetadata.data?.symbol ?? '';

	const coinDecimals = coinMetadata.data?.decimals ?? 0;
	const balanceInMist = new BigNumber(formattedBalance || '0')
		.times(MIST_PER_SUI.toString())
		.toString();

	const { data: estimateData } = useGetEstimateSuiToUSDC({ balanceInMist, signer, accountCapId });

	const validationSchema = useMemo(() => {
		return z.object({
			amount: z.string().transform((value, context) => {
				const bigNumberValue = new BigNumber(value);

				if (!value.length) {
					context.addIssue({
						code: 'custom',
						message: 'Amount is required.',
					});
					return z.NEVER;
				}

				if (bigNumberValue.lt(0)) {
					context.addIssue({
						code: 'custom',
						message: 'Amount must be greater than 0.',
					});
					return z.NEVER;
				}

				if (bigNumberValue.shiftedBy(coinDecimals).gt(BigInt(balanceInMist).toString())) {
					context.addIssue({
						code: 'custom',
						message: 'Not available in account',
					});
					return z.NEVER;
				}

				return value;
			}),
			isPayAll: z.boolean(),
			quoteAssetType: z.string(),
			allowedMaxSlippagePercentage: z.string().transform((percent, context) => {
				const numberPercent = Number(percent);

				if (numberPercent < 0 || numberPercent > 100) {
					context.addIssue({
						code: 'custom',
						message: 'Value must be between 0 and 100.',
					});
					return z.NEVER;
				}

				return percent;
			}),
		});
	}, [balanceInMist, coinDecimals]);

	const form = useZodForm({
		mode: 'all',
		schema: validationSchema,
		defaultValues: initialValues,
	});

	const {
		register,
		getValues,
		setValue,
		control,
		handleSubmit,
		trigger,
		formState: { isValid, isSubmitting, errors },
	} = form;

	const renderButtonToCoinsList = useMemo(() => {
		return (
			recognized.length > 1 &&
			recognized.some((coin) => allowedSwapCoinsList.includes(coin.coinType))
		);
	}, [recognized]);

	const isPayAll = getValues('isPayAll');
	const amount = useWatch({
		name: 'amount',
		control,
	});
	const quoteAssetType = getValues('quoteAssetType');
	const amountAsBigInt = new BigNumber(amount);
	const { rawValue } = useSuiBalanceInUSDC(amountAsBigInt);
	const atcText = useMemo(() => {
		return getSwapPageAtcText(symbol, quoteAssetType, coinsMap);
	}, [symbol, quoteAssetType, coinsMap]);

	const txnSummary = useTransactionSummary({
		transaction: estimateData as DryRunTransactionBlockResponse,
		recognizedPackagesList: [],
		currentAddress: activeAccountAddress,
	});

	const totalGas = txnSummary?.gas?.totalGas;

	useEffect(() => {
		if (!accountCapId && !useMarketAccountCapLoading) {
			mutate();
			refetchUseMarketAccountCapLoading();
		}
	}, [accountCapId, useMarketAccountCapLoading, mutate, refetchUseMarketAccountCapLoading]);

	const handleOnsubmit: SubmitHandler<FormValues> = async (data) => {};

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate('/')}>
			<div className="flex flex-col h-full w-full">
				<Loading loading={isLoading}>
					<BottomMenuLayout>
						<Content>
							<Form form={form} onSubmit={handleOnsubmit}>
								<div
									className={clsx(
										'flex flex-col border border-hero-darkest/20 rounded-xl pt-5 pb-6 px-5 gap-4 border-solid',
										isValid && 'bg-gradients-graph-cards',
									)}
								>
									{activeCoinType && (
										<AssetData
											disabled={!renderButtonToCoinsList}
											tokenBalance={formattedBalance}
											coinType={activeCoinType}
											symbol={symbol}
											to="/swap/base-assets"
										/>
									)}

									<InputWithActionButton
										{...register('amount')}
										dark
										value={amount}
										suffix={<div className="ml-2">{symbol}</div>}
										type="number"
										errorString={errors.amount?.message}
										actionText="Max"
										actionType="button"
										actionDisabled={isPayAll}
										onActionClicked={() => {
											setValue('amount', formattedTokenBalance);
											trigger('amount');
										}}
									/>

									{isValid && !!amount && (
										<div className="ml-3">
											<div className="text-bodySmall font-medium text-hero-darkest/40">
												{isPayAll ? '~ ' : ''}
												{getUSDCurrency(rawValue)}
											</div>
										</div>
									)}
								</div>

								<div className="flex my-4 gap-3 items-center">
									<div className="bg-gray-45 h-px w-full" />
									<div className="h-3 w-3">
										<ArrowDown12 className="text-steel" />
									</div>
									<div className="bg-gray-45 h-px w-full" />
								</div>

								<QuoteAssetSection />

								<div className="mt-4">
									<GasFeeSection totalGas={isValid ? totalGas : undefined} />
								</div>
							</Form>
						</Content>

						<Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0 gap-2.5">
							<Button
								onClick={handleSubmit(handleOnsubmit)}
								type="submit"
								variant="primary"
								loading={isSubmitting}
								disabled={!isValid || isSubmitting}
								size="tall"
								text={atcText}
								after={<ArrowRight16 />}
							/>
						</Menu>
					</BottomMenuLayout>
				</Loading>
			</div>
		</Overlay>
	);
}

export function SwapPage() {
	return (
		<ErrorBoundary>
			<Routes>
				<Route path="/" element={<SwapPageForm />} />
				<Route path="/base-assets" element={<BaseAssets />} />
			</Routes>
		</ErrorBoundary>
	);
}
