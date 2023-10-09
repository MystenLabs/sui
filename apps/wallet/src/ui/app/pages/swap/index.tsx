// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useRecognizedPackages } from '_app/hooks/useRecognizedPackages';
import { useSigner } from '_app/hooks/useSigner';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Form } from '_app/shared/forms/Form';
import { InputWithActionButton } from '_app/shared/InputWithAction';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import {
	allowedSwapCoinsList,
	Coins,
	getCoinsByBalance,
	getPlaceMarketOrderTxn,
	getUSDCurrency,
	SUI_CONVERSION_RATE,
	USDC_DECIMALS,
	useBalanceConversion,
	useCoinsReFetchingConfig,
	useGetEstimate,
	useMainnetCoinsMap,
	useMainnetPools,
	useSortedCoinsByCategories,
} from '_hooks';
import {
	DeepBookContextProvider,
	useDeepBookAccountCapId,
	useDeepBookClient,
} from '_shared/deepBook/context';
import { useFormatCoin, useTransactionSummary, useZodForm } from '@mysten/core';
import { useSuiClient, useSuiClientQuery } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16 } from '@mysten/icons';
import { type DryRunTransactionBlockResponse } from '@mysten/sui.js/client';
import { SUI_DECIMALS, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { useMemo } from 'react';
import { useWatch, type SubmitHandler } from 'react-hook-form';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { z } from 'zod';

import { AssetData } from './AssetData';
import { GasFeeSection } from './GasFeeSection';
import { QuoteAssetSection } from './QuoteAssetSection';
import { initialValues, type FormValues } from './utils';

function getSwapPageAtcText(
	fromSymbol: string,
	quoteAssetType: string,
	coinsMap: Record<string, string>,
) {
	const toSymbol =
		quoteAssetType === SUI_TYPE_ARG
			? Coins.SUI
			: Object.entries(coinsMap).find(([key, value]) => value === quoteAssetType)?.[0] || '';

	return `Swap ${fromSymbol} to ${toSymbol}`;
}

export function SwapPageContent() {
	const queryClient = useQueryClient();
	const suiClient = useSuiClient();
	const mainnetPools = useMainnetPools();
	const deepBookClient = useDeepBookClient();
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const activeAccount = useActiveAccount();
	const signer = useSigner(activeAccount);
	const activeAccountAddress = activeAccount?.address;
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();
	const coinsMap = useMainnetCoinsMap();

	const accountCapId = useDeepBookAccountCapId();

	const activeCoinType = searchParams.get('type');

	const { data: coinBalanceData, isLoading } = useSuiClientQuery(
		'getBalance',
		{ coinType: activeCoinType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const rawBalance = coinBalanceData?.totalBalance;

	const [formattedBalance, _, coinMetadata] = useFormatCoin(rawBalance, activeCoinType);

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
	const maxBalanceInMist = rawBalance || '0';

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

				if (bigNumberValue.shiftedBy(coinDecimals).gt(BigInt(maxBalanceInMist).toString())) {
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
	}, [maxBalanceInMist, coinDecimals]);

	const form = useZodForm({
		mode: 'all',
		schema: validationSchema,
		defaultValues: {
			...initialValues,
			quoteAssetType: coinsMap.USDC,
		},
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
	const quoteAssetType = useWatch({
		name: 'quoteAssetType',
		control,
	});
	const { rawValue } = useBalanceConversion(
		new BigNumber(amount),
		activeCoinType === SUI_TYPE_ARG ? Coins.SUI : Coins.USDC,
		activeCoinType === SUI_TYPE_ARG ? Coins.USDC : Coins.SUI,
		activeCoinType === SUI_TYPE_ARG ? -SUI_CONVERSION_RATE : SUI_CONVERSION_RATE,
	);

	const atcText = useMemo(() => {
		return getSwapPageAtcText(symbol, quoteAssetType, coinsMap);
	}, [symbol, quoteAssetType, coinsMap]);

	const balance = amount
		? new BigNumber(activeCoinType === SUI_TYPE_ARG ? amount : Math.floor(Number(rawValue || '0')))
				.shiftedBy(activeCoinType === SUI_TYPE_ARG ? SUI_DECIMALS : USDC_DECIMALS)
				.toString()
		: '0';

	const { data: currentEstimatedData } = useGetEstimate({
		balance,
		signer,
		accountCapId,
		coinType: activeCoinType || '',
		poolId: mainnetPools.SUI_USDC_2,
	});

	const recognizedPackagesList = useRecognizedPackages();

	const txnSummary = useTransactionSummary({
		transaction: currentEstimatedData as DryRunTransactionBlockResponse,
		recognizedPackagesList,
		currentAddress: activeAccountAddress,
	});

	const totalGas = txnSummary?.gas?.totalGas;

	const { mutate: handleSwap, isLoading: isSwapLoading } = useMutation({
		mutationFn: async () => {
			const data = await getCoinsByBalance({
				coinType: activeCoinType!,
				balance,
				suiClient,
				address: activeAccountAddress!,
			});

			const txn = await getPlaceMarketOrderTxn({
				deepBookClient,
				poolId: mainnetPools.SUI_USDC_2,
				balance,
				accountCapId,
				coins: data || [],
				coinType: activeCoinType!,
				address: activeAccountAddress!,
			});

			if (!txn || !signer) {
				throw new Error('Missing data');
			}

			return signer.signAndExecuteTransactionBlock({
				transactionBlock: txn,
				options: {
					showInput: true,
					showEffects: true,
					showEvents: true,
				},
			});
		},
		onSuccess: (response) => {
			queryClient.invalidateQueries(['get-coins']);
			queryClient.invalidateQueries(['coin-balance']);

			const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
				response.digest,
			)}&from=transactions`;
			return navigate(receiptUrl);
		},
	});

	const handleOnsubmit: SubmitHandler<FormValues> = async (formData) => {
		handleSwap();
	};

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
												{getUSDCurrency(
													activeCoinType === SUI_TYPE_ARG ? rawValue : Number(amount),
												)}
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

								<QuoteAssetSection
									activeCoinType={activeCoinType}
									balanceChanges={currentEstimatedData?.balanceChanges || []}
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
		<DeepBookContextProvider>
			<SwapPageContent />
		</DeepBookContextProvider>
	);
}
