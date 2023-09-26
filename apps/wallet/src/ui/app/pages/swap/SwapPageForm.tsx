// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { useAllBalances } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16, ChevronDown16 } from '@mysten/icons';
import { MIST_PER_SUI, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { Form, Formik } from 'formik';
import { useMemo } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { getUSDCurrency } from '_app/helpers/getUSDCurrency';
import { useSuiBalanceInUSDC } from '_app/hooks/useDeepbook';
import { Button } from '_app/shared/ButtonUI';
import { InputWithAction } from '_app/shared/InputWithAction';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import { IconButton } from '_components/IconButton';
import { CoinIcon } from '_components/coin-icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { validate } from '_pages/swap/validation';

export const initialValues = {
	amount: '',
	isPayAll: false,
};

export type FormValues = typeof initialValues;

function SuiToUSD({ amount, isPayAll }: { amount: string; isPayAll: boolean }) {
	const amountAsBigInt = new BigNumber(amount).times(MIST_PER_SUI.toString());
	const amountInUSD = useSuiBalanceInUSDC(amountAsBigInt);

	return (
		<Text variant="bodySmall" weight="medium" color="hero-darkest/40">
			{isPayAll ? '~ ' : ''}
			{getUSDCurrency(amountInUSD)}
		</Text>
	);
}

export function SwapPageForm() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');
	const selectedAddress = useActiveAddress();

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coins, isLoading: coinsLoading } = useAllBalances(
		{ owner: selectedAddress! },
		{
			enabled: !!selectedAddress,
			refetchInterval,
			staleTime,
			select: filterAndSortTokenBalances,
		},
	);

	const activeCoin = coins?.find(({ coinType }) => coinType === activeCoinType);
	const activeCoinBalance = activeCoin?.totalBalance;
	const [tokenBalance, symbol] = useFormatCoin(activeCoinBalance, activeCoinType);
	const formattedTokenBalance = tokenBalance.replace(/,/g, '');
	const coinMetadata = useCoinMetadata(activeCoinType);

	const coinDecimals = coinMetadata.data?.decimals ?? 0;
	const balanceInMist = new BigNumber(tokenBalance).times(MIST_PER_SUI.toString()).toString();

	const validationSchema = useMemo(() => {
		return validate(BigInt(balanceInMist), symbol, coinDecimals);
	}, [balanceInMist, coinDecimals, symbol]);

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate('/')}>
			<Loading loading={coinsLoading}>
				<Formik
					initialValues={initialValues}
					onSubmit={() => {}}
					validationSchema={validationSchema}
					enableReinitialize
					validateOnMount
					validateOnChange
				>
					{({ isValid, isSubmitting, setFieldValue, values, submitForm, validateField }) => {
						const newIsPayAll = !!values.amount && values.amount === tokenBalance;

						if (values.isPayAll !== newIsPayAll) {
							setFieldValue('isPayAll', newIsPayAll);
						}

						return (
							<BottomMenuLayout>
								<Content>
									<Form autoComplete="off" noValidate>
										<div
											className={clsx(
												'flex flex-col border border-hero-darkest/20 rounded-xl pt-5 pb-6 px-5 gap-4 border-solid',
												isValid && 'bg-gradients-graph-cards',
											)}
										>
											{activeCoinType && (
												<div className="flex justify-between items-center">
													<div className="flex gap-1 items-center">
														<CoinIcon coinType={activeCoinType} size="sm" />
														<Heading variant="heading6" weight="semibold" color="hero-dark">
															{symbol}
														</Heading>
														<IconButton
															variant="transparent"
															icon={<ChevronDown16 className="h-4 w-4 text-hero-dark" />}
															onClick={() => {
																navigate('/swap/base-assets');
															}}
														/>
													</div>
													<div>
														<Text variant="bodySmall" weight="medium" color="hero-darkest/40">
															{tokenBalance} {symbol}
														</Text>
													</div>
												</div>
											)}
											<InputWithAction
												type="numberInput"
												name="amount"
												placeholder="0.00"
												prefix={values.isPayAll ? '~ ' : ''}
												actionText="Max"
												suffix={` ${symbol}`}
												actionType="button"
												allowNegative={false}
												decimals
												rounded="lg"
												dark
												onActionClicked={async () => {
													// using await to make sure the value is set before the validation
													await setFieldValue('amount', formattedTokenBalance);

													validateField('amount');
												}}
											/>

											{isValid && activeCoinType === SUI_TYPE_ARG && (
												<SuiToUSD amount={values.amount} isPayAll={values.isPayAll} />
											)}
										</div>

										<div className="flex my-4 gap-3 items-center">
											<div className="bg-gray-45 h-px w-full" />
											<div className="h-3 w-3">
												<ArrowDown12 className="text-steel" />
											</div>
											<div className="bg-gray-45 h-px w-full" />
										</div>
									</Form>
								</Content>

								<Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0 gap-2.5">
									<Button
										type="submit"
										onClick={submitForm}
										variant="primary"
										loading={isSubmitting}
										disabled={!isValid || isSubmitting}
										size="tall"
										text="Review"
										after={<ArrowRight16 />}
									/>
								</Menu>
							</BottomMenuLayout>
						);
					}}
				</Formik>
			</Loading>
		</Overlay>
	);
}
