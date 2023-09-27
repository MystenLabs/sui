// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getUSDCurrency } from '_app/helpers/getUSDCurrency';
import { coinsMap, useSuiBalanceInUSDC } from '_app/hooks/useDeepbook';
import BottomMenuLayout, { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import { InputWithAction } from '_app/shared/InputWithAction';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import { CoinIcon } from '_components/coin-icon';
import { IconButton } from '_components/IconButton';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { validate } from '_pages/swap/validation';
import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { useAllBalances } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16, ChevronDown16 } from '@mysten/icons';
import { MIST_PER_SUI, SUI_TYPE_ARG } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { Form, Formik } from 'formik';
import { useMemo } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

export const initialValues = {
	amount: '',
	isPayAll: false,
};

export type FormValues = typeof initialValues;

function useActiveCoin(activeCoinType: string | null) {
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
	const [tokenBalance] = useFormatCoin(activeCoinBalance, activeCoinType);
	const coinMetadata = useCoinMetadata(activeCoinType);

	return {
		activeCoin,
		tokenBalance,
		coinMetadata,
		isLoading: coinsLoading || coinMetadata.isLoading,
	};
}

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

function AssetData({
	tokenBalance,
	coinType,
	symbol,
	to,
}: {
	tokenBalance: string;
	coinType: string;
	symbol: string;
	to: string;
}) {
	return (
		<div className="flex justify-between items-center">
			<div className="flex gap-1 items-center">
				<CoinIcon coinType={coinType} size="sm" />
				<ButtonOrLink to={to} className="flex gap-1 items-center no-underline">
					<Heading variant="heading6" weight="semibold" color="hero-dark">
						{symbol}
					</Heading>
					<ChevronDown16 className="h-4 w-4 text-hero-dark" />
				</ButtonOrLink>
			</div>
			{!!tokenBalance && (
				<Text variant="bodySmall" weight="medium" color="hero-darkest/40">
					{tokenBalance} {symbol}
				</Text>
			)}
		</div>
	);
}

function QuoteAssetSection() {
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');
	const quotedAssetType = searchParams.get('quoteAsset') || coinsMap.USDC;
	const { tokenBalance, coinMetadata } = useActiveCoin(quotedAssetType);
	const symbol = coinMetadata.data?.symbol ?? '';

	if (!coinMetadata.data) {
		return null;
	}

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid">
			<AssetData
				tokenBalance={tokenBalance}
				coinType={quotedAssetType}
				symbol={symbol}
				to={`/swap/quote-assets?type=${activeCoinType}`}
			/>
			<div className="py-2 pr-2 pl-3 rounded-lg bg-gray-40">
				<Text variant="body" weight="semibold" color="steel">
					--
				</Text>
			</div>
		</div>
	);
}

function GasFeeSection() {
	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid">
			<div className="flex justify-between">
				<Text variant="bodySmall" weight="medium" color="steel-dark">
					Fees (0.03%)
				</Text>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					--
				</Text>
			</div>

			<div className="bg-gray-40 h-px w-full" />

			<div className="flex justify-between">
				<Text variant="bodySmall" weight="medium" color="steel-dark">
					Estimated Gas Fee
				</Text>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					--
				</Text>
			</div>
		</div>
	);
}

export function SwapPageForm() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');
	const quotedAssetType = searchParams.get('quoteAsset') || coinsMap.USDC;

	const { isLoading, tokenBalance, coinMetadata } = useActiveCoin(activeCoinType);

	const formattedTokenBalance = tokenBalance.replace(/,/g, '');
	const symbol = coinMetadata.data?.symbol ?? '';

	const coinDecimals = coinMetadata.data?.decimals ?? 0;
	const balanceInMist = new BigNumber(tokenBalance || '0')
		.times(MIST_PER_SUI.toString())
		.toString();

	const validationSchema = useMemo(() => {
		return validate(BigInt(balanceInMist), symbol, coinDecimals);
	}, [balanceInMist, coinDecimals, symbol]);

	const atcText = useMemo(() => {
		let toSymbol = '';

		for (const key in coinsMap) {
			if (coinsMap[key as keyof typeof coinsMap] === quotedAssetType) {
				toSymbol = key;
				break;
			}
		}

		return `Swap ${symbol} to ${toSymbol}`;
	}, [symbol, quotedAssetType]);

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate('/')}>
			<div className="flex flex-col w-full h-full">
				<Loading loading={isLoading}>
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
													<AssetData
														tokenBalance={tokenBalance}
														coinType={activeCoinType}
														symbol={symbol}
														to="/swap/base-assets"
													/>
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

												{isValid && activeCoinType === SUI_TYPE_ARG && !!values.amount && (
													<div className="ml-3">
														<SuiToUSD amount={values.amount} isPayAll={values.isPayAll} />
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
												<GasFeeSection />
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
											text={atcText}
											after={<ArrowRight16 />}
										/>
									</Menu>
								</BottomMenuLayout>
							);
						}}
					</Formik>
				</Loading>
			</div>
		</Overlay>
	);
}
