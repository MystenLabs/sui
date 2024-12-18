// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useSigner } from '_app/hooks/useSigner';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Form } from '_app/shared/forms/Form';
import { Heading } from '_app/shared/heading';
import { InputWithActionButton } from '_app/shared/InputWithAction';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { AssetData } from '_pages/swap/AssetData';
import { GasFeesSummary } from '_pages/swap/GasFeesSummary';
import { MaxSlippage, MaxSlippageModal } from '_pages/swap/MaxSlippage';
import { useSwapTransaction } from '_pages/swap/useSwapTransaction';
import {
	DEFAULT_MAX_SLIPPAGE_PERCENTAGE,
	formatSwapQuote,
	maxSlippageFormSchema,
	useCoinTypesFromRouteParams,
	useGetBalance,
} from '_pages/swap/utils';
import { ampli } from '_shared/analytics/ampli';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { useBalanceInUSD, useCoinMetadata, useZodForm } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16 } from '@mysten/icons';
import { normalizeStructTag, SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';
import clsx from 'clsx';
import { useMemo, useState } from 'react';
import type { SubmitHandler } from 'react-hook-form';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { z } from 'zod';

export function SwapPage() {
	const navigate = useNavigate();
	const client = useSuiClient();
	const queryClient = useQueryClient();
	const activeAccount = useActiveAccount();
	const signer = useSigner(activeAccount);
	const [isSlippageModalOpen, setSlippageModalOpen] = useState(false);
	const [searchParams] = useSearchParams();
	const currentAddress = activeAccount?.address;
	const { fromCoinType, toCoinType } = useCoinTypesFromRouteParams();
	const defaultSlippage = useFeatureValue('defi-max-slippage', DEFAULT_MAX_SLIPPAGE_PERCENTAGE);
	const maxSlippage = Number(searchParams.get('maxSlippage') || defaultSlippage);
	const presetAmount = searchParams.get('presetAmount');
	const isSui = fromCoinType
		? normalizeStructTag(fromCoinType) === normalizeStructTag(SUI_TYPE_ARG)
		: false;
	const { data: fromCoinData } = useCoinMetadata(fromCoinType);

	const validationSchema = useMemo(() => {
		return z
			.object({
				amount: z
					.number({
						coerce: true,
						invalid_type_error: 'Input must be number only',
					})
					.pipe(z.coerce.string()),
			})
			.merge(maxSlippageFormSchema)
			.superRefine(async ({ amount }, ctx) => {
				if (!fromCoinType) {
					ctx.addIssue({
						code: z.ZodIssueCode.custom,
						message: 'Select a coin to swap from',
					});
					return z.NEVER;
				}

				const { totalBalance } = await client.getBalance({
					owner: currentAddress || '',
					coinType: fromCoinType,
				});
				const data = await client.getCoinMetadata({ coinType: fromCoinType });
				const bnAmount = new BigNumber(amount);
				const bnMaxBalance = new BigNumber(totalBalance || 0).shiftedBy(-1 * (data?.decimals ?? 0));

				if (bnAmount.isGreaterThan(bnMaxBalance)) {
					ctx.addIssue({
						path: ['amount'],
						code: z.ZodIssueCode.custom,
						message: 'Insufficient balance',
					});
					return z.NEVER;
				}

				if (!toCoinType) {
					ctx.addIssue({
						code: z.ZodIssueCode.custom,
						message: 'Select a coin to swap to',
					});
					return z.NEVER;
				}

				if (!bnAmount.isFinite() || !bnAmount.isPositive()) {
					ctx.addIssue({
						path: ['amount'],
						code: z.ZodIssueCode.custom,
						message: 'Expected a valid number',
					});
					return z.NEVER;
				}
				if (!bnAmount.gt(0)) {
					ctx.addIssue({
						path: ['amount'],
						code: z.ZodIssueCode.custom,
						message: 'Value must be greater than 0',
					});
					return z.NEVER;
				}
				if (!fromCoinType || !toCoinType) {
					return z.NEVER;
				}
			});
	}, [client, currentAddress, fromCoinType, toCoinType]);

	type FormType = z.infer<typeof validationSchema>;

	const form = useZodForm({
		mode: 'all',
		schema: validationSchema,
		defaultValues: {
			allowedMaxSlippagePercentage: maxSlippage,
			amount: presetAmount || '',
		},
	});

	const {
		watch,
		setValue,
		handleSubmit,
		register,
		reset,
		formState: { isValid: isFormValid, isSubmitting, errors },
	} = form;

	const [allowedMaxSlippagePercentage, amount] = watch(['allowedMaxSlippagePercentage', 'amount']);

	const { data: balance } = useGetBalance({
		coinType: fromCoinType!,
		owner: currentAddress,
	});

	const GAS_RESERVE = 0.1;
	const maxBalance = useMemo(() => {
		const bnBalance = new BigNumber(balance?.totalBalance || 0).shiftedBy(
			-1 * (fromCoinData?.decimals ?? 0),
		);
		return isSui && bnBalance.gt(GAS_RESERVE)
			? bnBalance
					.minus(GAS_RESERVE)
					.decimalPlaces(fromCoinData?.decimals ?? 0)
					.toString()
			: bnBalance.decimalPlaces(fromCoinData?.decimals ?? 0).toString();
	}, [balance?.totalBalance, fromCoinData?.decimals, isSui]);

	const { data: toCoinData } = useCoinMetadata(toCoinType);
	const fromCoinSymbol = fromCoinData?.symbol;
	const toCoinSymbol = toCoinData?.symbol;

	const parsed = parseAmount(amount || '0', fromCoinData?.decimals || 0);
	const isMaxBalance = new BigNumber(amount).isEqualTo(new BigNumber(maxBalance));
	const {
		data,
		isPending: swapTransactionPending,
		isLoading: swapTransactionLoading,
		refetch,
		error,
	} = useSwapTransaction({
		sender: currentAddress,
		fromType: fromCoinType || '',
		toType: toCoinType || '',
		amount: parsed.toString(),
		slippage: Number(allowedMaxSlippagePercentage),
		enabled: isFormValid && parsed > 0n && !!fromCoinType && !!toCoinType,
		source: 'sui-wallet',
	});

	const swapData = useMemo(() => {
		if (!data) return null;
		return formatSwapQuote({
			result: data,
			sender: currentAddress || '',
			fromType: fromCoinType || '',
			toType: toCoinType || '',
			fromCoinDecimals: fromCoinData?.decimals ?? 0,
			toCoinDecimals: toCoinData?.decimals ?? 0,
		});
	}, [currentAddress, fromCoinType, toCoinType, fromCoinData, toCoinData, data]);

	const toCoinBalanceInUSD = useBalanceInUSD(toCoinType || '', swapData?.toAmount ?? 0n);
	const inputAmountInUSD = useBalanceInUSD(fromCoinType || '', parsed || 0n);

	const { mutate: handleSwap, isPending: handleSwapPending } = useMutation({
		mutationFn: async (formData: FormType) => {
			const txn = swapData?.transaction;
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

			ampli.swappedCoin({
				fromCoinType: fromCoinType || '',
				toCoinType: toCoinType || '',
				totalBalance: Number(amount),
				estimatedReturnBalance: inputAmountInUSD || 0,
				provider: swapData?.provider,
			});

			const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
				response.digest,
			)}&from=transactions`;
			return navigate(receiptUrl);
		},
		onError: (error) => {
			ampli.swappedCoinFailed({
				estimatedReturnBalance: Number(swapData?.formattedToAmount || 0),
				fromCoinType: fromCoinType!,
				toCoinType: toCoinType!,
				totalBalance: Number(amount || 0),
				errorMessage: error.message,
				provider: swapData?.provider,
			});
		},
	});

	const handleOnsubmit: SubmitHandler<FormType> = (formData) => {
		handleSwap(formData);
	};

	const showGasFeeBanner = !swapTransactionPending && swapData && isSui && isMaxBalance;

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate('/')}>
			<div className="flex flex-col h-full w-full">
				<BottomMenuLayout>
					<Content>
						<Form form={form} onSubmit={handleOnsubmit}>
							<div
								className={clsx(
									'flex flex-col gap-4 border border-hero-darkest/20 rounded-xl p-5 border-solid',
									isFormValid && 'bg-gradients-graph-cards',
								)}
							>
								<AssetData
									coinType={fromCoinType || ''}
									to={`/swap/coins-select?${new URLSearchParams({
										toCoinType: toCoinType || '',
										source: 'fromCoinType',
										currentAmount: amount,
									})}`}
								/>
								<div>
									<InputWithActionButton
										{...register('amount')}
										suffix={fromCoinSymbol}
										noBorder={isFormValid}
										value={amount}
										type="number"
										errorString={errors.amount?.message}
										actionText="Max"
										actionType="button"
										actionDisabled={isMaxBalance}
										prefix={isMaxBalance ? '~' : undefined}
										info={
											isFormValid &&
											!!amount && (
												<Text variant="subtitleSmall" color="steel-dark">
													{isMaxBalance ? '~ ' : ''}$
													{new BigNumber(inputAmountInUSD || 0).toFixed(2)}
												</Text>
											)
										}
										onActionClicked={() => {
											setValue('amount', maxBalance, { shouldValidate: true });
										}}
									/>
								</div>
								{showGasFeeBanner && (
									<Alert mode="warning">
										<Text variant="pBodySmall">
											{GAS_RESERVE} {fromCoinSymbol} has been set aside to cover estimated max gas
											fees for this transaction
										</Text>
									</Alert>
								)}
							</div>

							<ButtonOrLink
								className="group flex my-4 gap-3 items-center w-full bg-transparent border-none cursor-pointer"
								onClick={() => {
									navigate(
										`/swap?${new URLSearchParams({
											type: toCoinType || '',
											toType: fromCoinType || '',
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

							<div className="flex flex-col gap-4">
								<div
									className={clsx(
										'flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid',
										{ 'bg-sui-primaryBlue2023/10': isFormValid },
									)}
								>
									<AssetData
										coinType={toCoinType || ''}
										to={`/swap/coins-select?${new URLSearchParams({
											fromCoinType: fromCoinType || '',
											source: 'toCoinType',
											currentAmount: amount,
										})}`}
									/>
									<div
										className={clsx(
											'flex h-[42px] items-center bg-gray-40 rounded-2lg px-3 py-2 border',
											{
												'border-solid border-hero-darkest/10': isFormValid,
												'border-transparent': !isFormValid,
											},
										)}
									>
										{swapTransactionLoading ? (
											<div className="flex items-center gap-1 text-steel">
												<LoadingIndicator color="inherit" />
												<Text variant="body" color="steel">
													Calculating...
												</Text>
											</div>
										) : (
											<div className="flex gap-2 items-center w-full">
												<Heading as="h5" variant="heading5" weight="semibold" color="steel-darker">
													{swapData?.formattedToAmount ?? 0}
												</Heading>
												<Text variant="body" weight="medium" color="steel">
													{toCoinSymbol}
												</Text>
												<div className="ml-auto">
													<Text variant="subtitleSmall" color="steel-dark">
														${new BigNumber(toCoinBalanceInUSD || 0).toFixed(2)}
													</Text>
												</div>
											</div>
										)}
									</div>

									<div className="ml-3">
										<MaxSlippage onOpen={() => setSlippageModalOpen(true)} />
									</div>
									<MaxSlippageModal
										isOpen={isSlippageModalOpen}
										onClose={() => {
											navigate(
												`/swap?${new URLSearchParams({
													type: fromCoinType || '',
													toType: toCoinType || '',
													maxSlippage: allowedMaxSlippagePercentage.toString(),
												}).toString()}`,
											);
											setSlippageModalOpen(false);
										}}
									/>

									{error && (
										<div className="flex flex-col gap-4">
											<Alert>
												<Text variant="pBody" weight="semibold">
													Calculation failed
												</Text>
												<Text variant="pBodySmall">
													{error.message || 'An error has occurred, try again.'}
												</Text>
											</Alert>
											<Button text="Recalculate" onClick={refetch} />
										</div>
									)}
								</div>

								{swapData?.estimatedRate && (
									<div className="flex flex-col border border-hero-darkest/20 rounded-xl px-5 py-3 gap-4 border-solid">
										<DescriptionItem title={<Text variant="bodySmall">Estimated Rate</Text>}>
											<Text variant="bodySmall" weight="medium" color="steel-darker">
												1 {fromCoinSymbol} ≈ {swapData?.estimatedRate} {toCoinSymbol}
											</Text>
										</DescriptionItem>
									</div>
								)}

								<GasFeesSummary
									transaction={swapData?.dryRunResponse}
									feePercentage={swapData?.feePercentage}
									accessFees={swapData?.accessFees}
									accessFeeType={swapData?.accessFeeType}
								/>
							</div>
						</Form>
					</Content>

					<Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0 gap-2.5">
						<Button
							onClick={handleSubmit(handleOnsubmit)}
							type="submit"
							variant="primary"
							loading={isSubmitting || handleSwapPending}
							disabled={
								!isFormValid ||
								isSubmitting ||
								swapTransactionLoading ||
								swapTransactionPending ||
								!!error
							}
							size="tall"
							text={
								fromCoinSymbol && toCoinSymbol
									? `Swap ${fromCoinSymbol} to ${toCoinSymbol}`
									: 'Swap'
							}
							after={<ArrowRight16 />}
						/>
					</Menu>
				</BottomMenuLayout>
			</div>
		</Overlay>
	);
}
