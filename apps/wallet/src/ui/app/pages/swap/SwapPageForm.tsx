// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	Coins,
	coinsMap,
	getUSDCurrency,
	useBalanceConversion,
	useSuiBalanceInUSDC,
} from '_app/hooks/useDeepbook';
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
import { QuoteAssets } from '_pages/swap/QuoteAssets';
import { validate } from '_pages/swap/validation';
import { useCoinMetadata, useFormatCoin } from '@mysten/core';
import { useAllBalances } from '@mysten/dapp-kit';
import { ArrowDown12, ArrowRight16, ChevronDown16, Refresh16 } from '@mysten/icons';
import { MIST_PER_SUI } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import clsx from 'classnames';
import { Form, Formik, useFormikContext } from 'formik';
import { useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

const FEES_PERCENTAGE = 0.03;

export const initialValues = {
	amount: '',
	isPayAll: false,
	quoteAssetType: coinsMap.USDC,
};

export type FormValues = typeof initialValues;

function useCoinTypeData(activeCoinType: string | null) {
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
	const amountAsBigInt = new BigNumber(amount);
	const { rawValue } = useSuiBalanceInUSDC(amountAsBigInt);

	return (
		<Text variant="bodySmall" weight="medium" color="hero-darkest/40">
			{isPayAll ? '~ ' : ''}
			{getUSDCurrency(rawValue)}
		</Text>
	);
}

function AssetData({
	tokenBalance,
	coinType,
	symbol,
	to,
	onClick,
}: {
	tokenBalance: string;
	coinType: string;
	symbol: string;
	to?: string;
	onClick?: () => void;
}) {
	return (
		<div className="flex justify-between items-center">
			<div className="flex gap-1 items-center">
				<CoinIcon coinType={coinType} size="sm" />
				<ButtonOrLink
					onClick={onClick}
					to={to}
					className="flex gap-1 items-center no-underline outline-none border-transparent bg-transparent cursor-pointer p-0"
				>
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

function getCoinFromSymbol(symbol: string) {
	switch (symbol) {
		case 'SUI':
			return Coins.SUI;
		case 'USDC':
			return Coins.USDC;
		case 'USDT':
			return Coins.USDT;
		case 'WETH':
			return Coins.WETH;
		case 'tBTC':
			return Coins.tBTC;
		default:
			return Coins.SUI;
	}
}

function QuoteAssetSection() {
	const [isQuoteAssetOpen, setQuoteAssetOpen] = useState(false);
	const { values, isValid, setFieldValue } = useFormikContext<FormValues>();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');
	const { data: activeCoinData } = useCoinMetadata(activeCoinType);
	const quoteAssetType = values.quoteAssetType;
	const { tokenBalance: quoteAssetBalance, coinMetadata: quotedAssetMetaData } =
		useCoinTypeData(quoteAssetType);
	const quotedAssetSymbol = quotedAssetMetaData.data?.symbol ?? '';

	const { rawValue, averagePrice, refetch, isRefetching } = useBalanceConversion(
		new BigNumber(values.amount),
		getCoinFromSymbol(activeCoinData?.symbol ?? 'SUI'),
		getCoinFromSymbol(quotedAssetSymbol),
	);

	const averagePriceAsString = String(averagePrice);

	const { rawValue: rawValueQuoteToUsd } = useBalanceConversion(
		new BigNumber(rawValue || 0),
		getCoinFromSymbol(quotedAssetSymbol),
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
				isOpen={isQuoteAssetOpen}
				setOpen={setQuoteAssetOpen}
				onRowClick={(coinType) => {
					setQuoteAssetOpen(false);
					setFieldValue('quoteAssetType', coinType);
				}}
			/>
			<AssetData
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
				{rawValue && (
					<Text variant="body" weight="semibold" color="steel-darker">
						{rawValue.toLocaleString()}
					</Text>
				)}
				<Text variant="body" weight="semibold" color="steel">
					{rawValue ? quotedAssetSymbol : '--'}
				</Text>
			</div>
			{rawValue && (
				<div className="ml-3">
					<div className="flex justify-between flex-wrap items-center gap-2">
						<Text variant="bodySmall" weight="medium" color="steel-dark">
							{isRefetching ? '--' : getUSDCurrency(rawValueQuoteToUsd)}
						</Text>
						<div className="flex gap-1 items-center">
							<Text variant="bodySmall" weight="medium" color="steel-dark">
								1 {activeCoinData?.symbol} = {isRefetching ? '--' : averagePriceAsString}{' '}
								{quotedAssetSymbol}
							</Text>
							<IconButton
								icon={<Refresh16 className="h-4 w-4 text-steeldark hover:text-hero-dark" />}
								onClick={() => refetch()}
								loading={isRefetching}
							/>
						</div>
					</div>
				</div>
			)}
		</div>
	);
}

function GasFeeSection() {
	const { values, isValid } = useFormikContext<FormValues>();
	const [searchParams] = useSearchParams();

	const activeCoinType = searchParams.get('type');

	const { data: activeCoinData } = useCoinMetadata(activeCoinType);

	const amount = values.amount;

	const estimatedFess = useMemo(() => {
		if (!amount || !isValid) {
			return null;
		}

		return new BigNumber(amount).times(FEES_PERCENTAGE);
	}, [amount, isValid]);

	const estimatedFessAsBigInt = estimatedFess ? new BigNumber(estimatedFess) : null;

	// TODO: need to handle for all coins
	const { rawValue } = useSuiBalanceInUSDC(estimatedFessAsBigInt);

	const formattedEstimatedFees = getUSDCurrency(rawValue);

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl p-5 gap-4 border-solid">
			<div className="flex justify-between">
				<Text variant="bodySmall" weight="medium" color="steel-dark">
					Fees ({FEES_PERCENTAGE}%)
				</Text>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{estimatedFess
						? `${estimatedFess.toLocaleString()} ${activeCoinData?.symbol} (${formattedEstimatedFees})`
						: '--'}
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

function getSwapPageAtcText(fromSymbol: string, quoteAssetType: string) {
	let toSymbol = '';

	for (const key in coinsMap) {
		if (coinsMap[key as keyof typeof coinsMap] === quoteAssetType) {
			toSymbol = key;
			break;
		}
	}

	return `Swap ${fromSymbol} to ${toSymbol}`;
}

export function SwapPageForm() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();

	const activeCoinType = searchParams.get('type');

	const { isLoading, tokenBalance, coinMetadata } = useCoinTypeData(activeCoinType);

	const formattedTokenBalance = tokenBalance.replace(/,/g, '');
	const symbol = coinMetadata.data?.symbol ?? '';

	const coinDecimals = coinMetadata.data?.decimals ?? 0;
	const balanceInMist = new BigNumber(tokenBalance || '0')
		.times(MIST_PER_SUI.toString())
		.toString();

	const validationSchema = useMemo(() => {
		return validate(BigInt(balanceInMist), symbol, coinDecimals);
	}, [balanceInMist, coinDecimals, symbol]);

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
								<>
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

													{isValid && !!values.amount && (
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
												text={getSwapPageAtcText(symbol, values.quoteAssetType)}
												after={<ArrowRight16 />}
											/>
										</Menu>
									</BottomMenuLayout>
								</>
							);
						}}
					</Formik>
				</Loading>
			</div>
		</Overlay>
	);
}
