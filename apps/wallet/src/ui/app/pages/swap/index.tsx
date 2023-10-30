// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useRecognizedPackages } from '_app/hooks/useRecognizedPackages';
import { useSigner } from '_app/hooks/useSigner';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Form } from '_app/shared/forms/Form';
import { InputWithActionButton } from '_app/shared/InputWithAction';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import {
	allowedSwapCoinsList,
	Coins,
	useCoinsReFetchingConfig,
	useDeepBookConfigs,
	useGetEstimate,
	useSortedCoinsByCategories,
} from '_hooks';
import {
	initialValues,
	SUI_CONVERSION_RATE,
	USDC_DECIMALS,
	type FormValues,
} from '_pages/swap/constants';
import {
	getUSDCurrency,
	isExceedingSlippageTolerance,
	useSuiUsdcBalanceConversion,
	useSwapData,
} from '_pages/swap/utils';
import { DeepBookContextProvider, useDeepBookContext } from '_shared/deepBook/context';
import { useTransactionSummary, useZodForm } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16 } from '@mysten/icons';
import { type BalanceChange, type DryRunTransactionBlockResponse } from '@mysten/sui.js/client';
import { SUI_DECIMALS, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { useEffect, useMemo, useState } from 'react';
import { useWatch, type SubmitHandler } from 'react-hook-form';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { z } from 'zod';

import { AssetData } from './AssetData';
import { GasFeeSection } from './GasFeeSection';
import { ToAssetSection } from './ToAssetSection';

enum ErrorStrings {
	MISSING_DATA = 'Missing data',
	SLIPPAGE_EXCEEDS_TOLERANCE = 'Current slippage exceeds tolerance',
	NOT_ENOUGH_BALANCE = 'Not enough balance',
}

function getSwapPageAtcText(
	fromSymbol: string,
	toAssetType: string,
	coinsMap: Record<string, string>,
) {
	const toSymbol =
		toAssetType === SUI_TYPE_ARG
			? Coins.SUI
			: Object.entries(coinsMap).find(([key, value]) => value === toAssetType)?.[0] || '';

	return `Swap ${fromSymbol} to ${toSymbol}`;
}

function getCoinsFromBalanceChanges(coinType: string, balanceChanges: BalanceChange[]) {
	return balanceChanges
		.filter((balance) => {
			return balance.coinType === coinType;
		})
		.sort((a, b) => {
			const aAmount = new BigNumber(a.amount).abs();
			const bAmount = new BigNumber(b.amount).abs();

			return aAmount.isGreaterThan(bAmount) ? -1 : 1;
		});
}

export function SwapPageContent() {
	const [slippageErrorString, setSlippageErrorString] = useState('');
	const queryClient = useQueryClient();
	const mainnetPools = useDeepBookConfigs().pools;
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const activeAccount = useActiveAccount();
	const signer = useSigner(activeAccount);
	const activeAccountAddress = activeAccount?.address;
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();
	const coinsMap = useDeepBookConfigs().coinsMap;
	const deepBookClient = useDeepBookContext().client;

	const accountCapId = useDeepBookContext().accountCapId;

	const activeCoinType = searchParams.get('type');
	const isAsk = activeCoinType === SUI_TYPE_ARG;

	const baseCoinType = SUI_TYPE_ARG;
	const quoteCoinType = coinsMap.USDC;

	const poolId = mainnetPools.SUI_USDC[0];

	const {
		baseCoinBalanceData,
		quoteCoinBalanceData,
		formattedBaseBalance,
		formattedQuoteBalance,
		baseCoinMetadata,
		quoteCoinMetadata,
		baseCoinSymbol,
		quoteCoinSymbol,
		isPending,
	} = useSwapData({
		baseCoinType,
		quoteCoinType,
		activeCoinType: activeCoinType || '',
	});

	const rawBaseBalance = baseCoinBalanceData?.totalBalance;
	const rawQuoteBalance = quoteCoinBalanceData?.totalBalance;

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

	const formattedBaseTokenBalance = formattedBaseBalance.replace(/,/g, '');

	const formattedQuoteTokenBalance = formattedQuoteBalance.replace(/,/g, '');

	const baseCoinDecimals = baseCoinMetadata.data?.decimals ?? 0;
	const maxBaseBalance = rawBaseBalance || '0';

	const quoteCoinDecimals = quoteCoinMetadata.data?.decimals ?? 0;
	const maxQuoteBalance = rawQuoteBalance || '0';

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

				const shiftedValue = isAsk ? baseCoinDecimals : quoteCoinDecimals;
				const maxBalance = isAsk ? maxBaseBalance : maxQuoteBalance;

				if (bigNumberValue.shiftedBy(shiftedValue).gt(BigInt(maxBalance).toString())) {
					context.addIssue({
						code: 'custom',
						message: 'Not available in account',
					});
					return z.NEVER;
				}

				return value;
			}),
			toAssetType: z.string(),
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
	}, [isAsk, baseCoinDecimals, quoteCoinDecimals, maxBaseBalance, maxQuoteBalance]);

	const form = useZodForm({
		mode: 'all',
		schema: validationSchema,
		defaultValues: {
			...initialValues,
			toAssetType: coinsMap.USDC,
		},
	});

	const {
		register,
		setValue,
		control,
		handleSubmit,
		reset,
		formState: { isValid, isSubmitting, errors, isDirty },
	} = form;

	useEffect(() => {
		if (isDirty) {
			setSlippageErrorString('');
		}
	}, [isDirty]);

	const renderButtonToCoinsList = useMemo(() => {
		return (
			recognized.length > 1 &&
			recognized.some((coin) => allowedSwapCoinsList.includes(coin.coinType))
		);
	}, [recognized]);

	const amount = useWatch({
		name: 'amount',
		control,
	});

	const isPayAll = amount === (isAsk ? formattedBaseTokenBalance : formattedQuoteTokenBalance);

	const { suiUsdc, usdcSui } = useSuiUsdcBalanceConversion({ amount });
	const rawInputSuiUsdc = suiUsdc.data?.rawValue;
	const rawInputUsdcSui = usdcSui.data?.rawValue;

	const atcText = useMemo(() => {
		if (isAsk) {
			return getSwapPageAtcText(baseCoinSymbol, quoteCoinType, coinsMap);
		}
		return getSwapPageAtcText(quoteCoinSymbol, baseCoinType, coinsMap);
	}, [isAsk, baseCoinSymbol, baseCoinType, coinsMap, quoteCoinSymbol, quoteCoinType]);

	const baseBalance = new BigNumber(isAsk ? amount || 0 : rawInputUsdcSui || 0)
		.shiftedBy(SUI_DECIMALS)
		.toString();
	const quoteBalance = new BigNumber(isAsk ? rawInputSuiUsdc || 0 : amount || 0)
		.shiftedBy(SUI_CONVERSION_RATE)
		.toString();

	const {
		data: dataFromEstimate,
		isPending: dataFromEstimateLoading,
		isError: dataFromEstimateError,
	} = useGetEstimate({
		signer,
		accountCapId,
		coinType: activeCoinType || '',
		poolId,
		baseBalance,
		quoteBalance,
		isAsk,
	});

	const recognizedPackagesList = useRecognizedPackages();

	const txnSummary = useTransactionSummary({
		transaction: dataFromEstimate?.dryRunResponse as DryRunTransactionBlockResponse,
		recognizedPackagesList,
		currentAddress: activeAccountAddress,
	});

	const totalGas = txnSummary?.gas?.totalGas;
	const balanceChanges = dataFromEstimate?.dryRunResponse?.balanceChanges || [];

	const { mutate: handleSwap, isPending: isSwapLoading } = useMutation({
		mutationFn: async (formData: FormValues) => {
			const txn = dataFromEstimate?.txn;

			const baseCoins = getCoinsFromBalanceChanges(baseCoinType, balanceChanges);
			const quoteCoins = getCoinsFromBalanceChanges(quoteCoinType, balanceChanges);

			const baseCoinAmount = baseCoins[0]?.amount;
			const quoteCoinAmount = quoteCoins[0]?.amount;

			if (!baseCoinAmount || !quoteCoinAmount) {
				throw new Error(ErrorStrings.MISSING_DATA);
			}

			const isExceedingSlippage = await isExceedingSlippageTolerance({
				slipPercentage: formData.allowedMaxSlippagePercentage,
				poolId,
				deepBookClient,
				conversionRate: USDC_DECIMALS,
				baseCoinAmount,
				quoteCoinAmount,
				isAsk,
			});

			if (!balanceChanges.length) {
				throw new Error(ErrorStrings.NOT_ENOUGH_BALANCE);
			}

			if (isExceedingSlippage) {
				throw new Error(ErrorStrings.SLIPPAGE_EXCEEDS_TOLERANCE);
			}

			if (!txn || !signer) {
				throw new Error(ErrorStrings.MISSING_DATA);
			}

			return signer!.signAndExecuteTransactionBlock({
				transactionBlock: txn!,
				options: {
					showInput: true,
					showEffects: true,
					showEvents: true,
				},
			});
		},
		onSuccess: (response) => {
			queryClient.invalidateQueries({ queryKey: ['get-coins'] });
			queryClient.invalidateQueries({ queryKey: ['coin-balance'] });

			const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
				response.digest,
			)}&from=transactions`;
			return navigate(receiptUrl);
		},
		onError: (error: Error) => {
			if (error.message === ErrorStrings.SLIPPAGE_EXCEEDS_TOLERANCE) {
				setSlippageErrorString(error.message);
			}
		},
	});

	const handleOnsubmit: SubmitHandler<FormValues> = (formData) => {
		handleSwap(formData);
	};

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate('/')}>
			<div className="flex flex-col h-full w-full">
				<Loading loading={isPending}>
					<BottomMenuLayout>
						<Content>
							<Form form={form} onSubmit={handleOnsubmit}>
								<div
									className={clsx(
										'flex flex-col border border-hero-darkest/20 rounded-xl pt-5 pb-6 px-5 border-solid',
										isValid && 'bg-gradients-graph-cards',
									)}
								>
									{activeCoinType && (
										<AssetData
											disabled={!renderButtonToCoinsList}
											tokenBalance={isAsk ? formattedBaseTokenBalance : formattedQuoteTokenBalance}
											coinType={activeCoinType}
											symbol={isAsk ? baseCoinSymbol : quoteCoinSymbol}
											to="/swap/from-assets"
										/>
									)}

									<div className="mt-4">
										<InputWithActionButton
											{...register('amount')}
											dark
											suffix={isAsk ? baseCoinSymbol : quoteCoinSymbol}
											value={amount}
											type="number"
											errorString={errors.amount?.message}
											actionText="Max"
											actionType="button"
											actionDisabled={isPayAll}
											prefix={isPayAll ? '~' : undefined}
											onActionClicked={() => {
												setValue(
													'amount',
													activeCoinType === SUI_TYPE_ARG
														? formattedBaseTokenBalance
														: formattedQuoteTokenBalance,
													{ shouldDirty: true },
												);
											}}
										/>
									</div>

									{isValid && !!amount && (
										<div className="ml-3 mt-3">
											<div className="text-bodySmall font-medium text-hero-darkest/40">
												{isPayAll ? '~ ' : ''}
												{getUSDCurrency(isAsk ? rawInputSuiUsdc : Number(amount))}
											</div>
										</div>
									)}
								</div>

								<ButtonOrLink
									className="group flex my-4 gap-3 items-center w-full bg-transparent border-none cursor-pointer"
									onClick={() => {
										navigate(
											`/swap?${new URLSearchParams({
												type: activeCoinType === SUI_TYPE_ARG ? coinsMap.USDC : SUI_TYPE_ARG,
											}).toString()}`,
										);
										reset();
									}}
								>
									<div className="bg-gray-45 h-px w-full group-hover:bg-hero-dark" />
									<div className="h-3 w-3">
										<ArrowDown12 className="text-steel group-hover:text-hero-dark" />
									</div>
									<div className="bg-gray-45 h-px w-full group-hover:bg-hero-dark" />
								</ButtonOrLink>

								<ToAssetSection
									slippageErrorString={slippageErrorString}
									activeCoinType={activeCoinType}
									balanceChanges={balanceChanges}
									baseCoinType={baseCoinType}
									quoteCoinType={quoteCoinType}
								/>

								<div className="mt-4">
									<GasFeeSection
										totalGas={totalGas || ''}
										activeCoinType={activeCoinType}
										amount={amount}
										isValid={isValid}
									/>
								</div>
							</Form>
						</Content>

						<Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0 gap-2.5">
							<Button
								onClick={handleSubmit(handleOnsubmit)}
								type="submit"
								variant="primary"
								loading={isSubmitting || isSwapLoading}
								disabled={
									!isValid || isSubmitting || dataFromEstimateLoading || dataFromEstimateError
								}
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
		<DeepBookContextProvider>
			<SwapPageContent />
		</DeepBookContextProvider>
	);
}
