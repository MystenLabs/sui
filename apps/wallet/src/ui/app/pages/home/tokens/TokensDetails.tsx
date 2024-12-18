// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '_app/hooks/useAppResolveSuinsName';
import { useIsWalletDefiEnabled } from '_app/hooks/useIsWalletDefiEnabled';
import { LargeButton } from '_app/shared/LargeButton';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import Alert from '_components/alert';
import { CoinIcon } from '_components/coin-icon';
import Loading from '_components/loading';
import { filterAndSortTokenBalances } from '_helpers';
import { useAppSelector, useCoinsReFetchingConfig, useSortedCoinsByCategories } from '_hooks';
import { UsdcPromoBanner } from '_pages/home/usdc-promo/UsdcPromoBanner';
import {
	DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	DELEGATED_STAKES_QUERY_STALE_TIME,
} from '_shared/constants';
import { ampli } from '_src/shared/analytics/ampli';
import { API_ENV } from '_src/shared/api-env';
import { FEATURES } from '_src/shared/experimentation/features';
import { AccountsList } from '_src/ui/app/components/accounts/AccountsList';
import { UnlockAccountButton } from '_src/ui/app/components/accounts/UnlockAccountButton';
import { BuyNLargeHomePanel } from '_src/ui/app/components/buynlarge/HomePanel';
import { useActiveAccount } from '_src/ui/app/hooks/useActiveAccount';
import { useCoinMetadataOverrides } from '_src/ui/app/hooks/useCoinMetadataOverride';
import { usePinnedCoinTypes } from '_src/ui/app/hooks/usePinnedCoinTypes';
import FaucetRequestButton from '_src/ui/app/shared/faucet/FaucetRequestButton';
import PageTitle from '_src/ui/app/shared/PageTitle';
import { useFeature } from '@growthbook/growthbook-react';
import {
	useAppsBackend,
	useBalanceInUSD,
	useCoinMetadata,
	useFormatCoin,
	useGetDelegatedStake,
} from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { Info12, Pin16, Unpin16 } from '@mysten/icons';
import { type CoinBalance as CoinBalanceType } from '@mysten/sui/client';
import { formatAddress, parseStructTag, SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';
import clsx from 'clsx';
import { useEffect, useState, type ReactNode } from 'react';

import Interstitial, { type InterstitialConfig } from '../interstitial';
import { useOnrampProviders } from '../onramp/useOnrampProviders';
import { CoinBalance } from './coin-balance';
import { PortfolioName } from './PortfolioName';
import { TokenIconLink } from './TokenIconLink';
import { TokenLink } from './TokenLink';
import { TokenList } from './TokenList';

type TokenDetailsProps = {
	coinType?: string;
};

function PinButton({ unpin, onClick }: { unpin?: boolean; onClick: () => void }) {
	return (
		<button
			type="button"
			className="border-none bg-transparent text-transparent group-hover/coin:text-steel hover:!text-hero cursor-pointer"
			aria-label={unpin ? 'Unpin Coin' : 'Pin Coin'}
			onClick={(e) => {
				e.preventDefault();
				e.stopPropagation();
				onClick();
			}}
		>
			{unpin ? <Unpin16 /> : <Pin16 />}
		</button>
	);
}

function TokenRowButton({
	coinBalance,
	children,
	to,
	onClick,
}: {
	coinBalance: CoinBalanceType;
	children: ReactNode;
	to: string;
	onClick?: () => void;
}) {
	return (
		<ButtonOrLink
			to={to}
			key={coinBalance.coinType}
			onClick={onClick}
			className="no-underline text-subtitle font-medium text-steel hover:font-semibold hover:text-hero"
		>
			{children}
		</ButtonOrLink>
	);
}

export function TokenRow({
	coinBalance,
	renderActions,
	onClick,
}: {
	coinBalance: CoinBalanceType;
	renderActions?: boolean;
	onClick?: () => void;
}) {
	const coinType = coinBalance.coinType;
	const balance = BigInt(coinBalance.totalBalance);
	const [formatted, symbol, { data: coinMeta }] = useFormatCoin(balance, coinType);
	const Tag = onClick ? 'button' : 'div';
	const params = new URLSearchParams({
		type: coinBalance.coinType,
	});

	const balanceInUsd = useBalanceInUSD(coinBalance.coinType, coinBalance.totalBalance);

	const coinMetadataOverrides = useCoinMetadataOverrides();
	return (
		<Tag
			className={clsx(
				'group flex py-3 pl-1.5 pr-2 rounded hover:bg-sui/10 items-center bg-transparent border-transparent',
				onClick && 'hover:cursor-pointer',
			)}
			onClick={onClick}
		>
			<div className="flex gap-2.5">
				<CoinIcon coinType={coinType} size="md" />
				<div className="flex flex-col gap-1 items-start">
					<Text variant="body" color="gray-90" weight="semibold" truncate>
						{coinMetadataOverrides[coinBalance.coinType]?.name || coinMeta?.name || symbol}
					</Text>

					{renderActions && (
						<div className="group-hover:hidden">
							<Text variant="subtitle" color="steel-dark" weight="medium">
								{symbol}
							</Text>
						</div>
					)}

					{renderActions ? (
						<div className="group-hover:flex hidden gap-2.5 items-center">
							<TokenRowButton
								coinBalance={coinBalance}
								to={`/send?${params.toString()}`}
								onClick={() =>
									ampli.selectedCoin({
										coinType: coinBalance.coinType,
										totalBalance: Number(formatted),
										sourceFlow: 'TokenDetails',
									})
								}
							>
								Send
							</TokenRowButton>
							<TokenRowButton
								coinBalance={coinBalance}
								to={`/swap?${params.toString()}`}
								onClick={() => {
									ampli.clickedSwapCoin({
										coinType: coinBalance.coinType,
										totalBalance: Number(formatted),
										sourceFlow: 'TokenRow',
									});
								}}
							>
								Swap
							</TokenRowButton>
						</div>
					) : (
						<div className="flex flex-wrap gap-1 items-start">
							<Text variant="subtitleSmall" weight="semibold" color="gray-90">
								{symbol}
							</Text>
							<Text variant="subtitleSmall" weight="medium" color="steel-dark">
								{formatAddress(coinType)}
							</Text>
						</div>
					)}
				</div>
			</div>

			<div className="ml-auto flex flex-col items-end gap-1">
				{balance > 0n && (
					<Text variant="body" color="gray-90" weight="medium" className="text-end">
						{formatted} {symbol}
					</Text>
				)}

				{balanceInUsd && balanceInUsd > 0 ? (
					<Text variant="subtitle" color="steel-dark" weight="medium">
						{Number(balanceInUsd).toLocaleString('en', {
							style: 'currency',
							currency: 'USD',
						})}
					</Text>
				) : null}
			</div>
		</Tag>
	);
}

export function MyTokens({
	coinBalances,
	isLoading,
	isFetched,
}: {
	coinBalances: CoinBalanceType[];
	isLoading: boolean;
	isFetched: boolean;
}) {
	const isDefiWalletEnabled = useIsWalletDefiEnabled();
	const apiEnv = useAppSelector(({ app }) => app.apiEnv);

	const [_, { pinCoinType, unpinCoinType }] = usePinnedCoinTypes();

	const { recognized, pinned, unrecognized } = useSortedCoinsByCategories(coinBalances);

	// Avoid perpetual loading state when fetching and retry keeps failing; add isFetched check.
	const isFirstTimeLoading = isLoading && !isFetched;

	return (
		<Loading loading={isFirstTimeLoading}>
			{recognized.length > 0 && (
				<TokenList title="My Coins" defaultOpen>
					{recognized.map((coinBalance) =>
						isDefiWalletEnabled ? (
							<TokenRow renderActions key={coinBalance.coinType} coinBalance={coinBalance} />
						) : (
							<TokenLink key={coinBalance.coinType} coinBalance={coinBalance} />
						),
					)}
				</TokenList>
			)}

			{pinned.length > 0 && (
				<TokenList title="Pinned Coins" defaultOpen>
					{pinned.map((coinBalance) => (
						<TokenLink
							key={coinBalance.coinType}
							coinBalance={coinBalance}
							centerAction={
								<PinButton
									unpin
									onClick={() => {
										ampli.unpinnedCoin({ coinType: coinBalance.coinType });
										unpinCoinType(coinBalance.coinType);
									}}
								/>
							}
						/>
					))}
				</TokenList>
			)}

			{unrecognized.length > 0 && (
				<TokenList
					title={
						unrecognized.length === 1
							? `${unrecognized.length} Unrecognized Coin`
							: `${unrecognized.length} Unrecognized Coins`
					}
					defaultOpen={apiEnv !== API_ENV.mainnet}
				>
					{unrecognized.map((coinBalance) => (
						<TokenLink
							key={coinBalance.coinType}
							coinBalance={coinBalance}
							subtitle="Send"
							centerAction={
								<PinButton
									onClick={() => {
										ampli.pinnedCoin({ coinType: coinBalance.coinType });
										pinCoinType(coinBalance.coinType);
									}}
								/>
							}
						/>
					))}
				</TokenList>
			)}
		</Loading>
	);
}

function getMostNestedName(parsed: ReturnType<typeof parseStructTag>) {
	if (parsed.typeParams.length === 0) {
		return parsed.name;
	}

	if (typeof parsed.typeParams[0] === 'string') {
		return parsed.typeParams[0];
	}

	return getMostNestedName(parsed.typeParams[0]);
}

function getFallbackSymbol(coinType: string) {
	const parsed = parseStructTag(coinType);
	return getMostNestedName(parsed);
}

function TokenDetails({ coinType }: TokenDetailsProps) {
	const isDefiWalletEnabled = useIsWalletDefiEnabled();
	const [interstitialDismissed, setInterstitialDismissed] = useState<boolean>(false);
	const activeCoinType = coinType || SUI_TYPE_ARG;
	const activeAccount = useActiveAccount();
	const activeAccountAddress = activeAccount?.address;
	const domainName = useResolveSuiNSName(activeAccountAddress);

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();
	const {
		data: coinBalance,
		isError,
		isPending,
		isFetched,
	} = useSuiClientQuery(
		'getBalance',
		{ coinType: activeCoinType, owner: activeAccountAddress! },
		{ enabled: !!activeAccountAddress, refetchInterval, staleTime },
	);

	const { apiEnv } = useAppSelector((state) => state.app);
	const isMainnet = apiEnv === API_ENV.mainnet;
	const { request } = useAppsBackend();
	const { data } = useQuery({
		queryKey: ['apps-backend', 'monitor-network'],
		queryFn: () =>
			request<{ degraded: boolean }>('monitor-network', {
				project: 'WALLET',
			}),
		// Keep cached for 2 minutes:
		staleTime: 2 * 60 * 1000,
		retry: false,
		enabled: isMainnet,
	});

	const {
		data: coinBalances,
		isPending: coinBalancesLoading,
		isFetched: coinBalancesFetched,
	} = useSuiClientQuery(
		'getAllBalances',
		{ owner: activeAccountAddress! },
		{
			enabled: !!activeAccountAddress,
			staleTime,
			refetchInterval,
			select: filterAndSortTokenBalances,
		},
	);

	const { data: delegatedStake } = useGetDelegatedStake({
		address: activeAccountAddress || '',
		staleTime: DELEGATED_STAKES_QUERY_STALE_TIME,
		refetchInterval: DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	});

	const walletInterstitialConfig = useFeature<InterstitialConfig>(
		FEATURES.WALLET_INTERSTITIAL_CONFIG,
	).value;

	const { providers } = useOnrampProviders();

	const tokenBalance = BigInt(coinBalance?.totalBalance ?? 0);
	const [formatted] = useFormatCoin(tokenBalance, activeCoinType);

	const { data: coinMetadata } = useCoinMetadata(activeCoinType);
	const coinSymbol = coinMetadata ? coinMetadata.symbol : getFallbackSymbol(activeCoinType);

	// Avoid perpetual loading state when fetching and retry keeps failing add isFetched check
	const isFirstTimeLoading = isPending && !isFetched;

	useEffect(() => {
		const dismissed =
			walletInterstitialConfig?.dismissKey &&
			localStorage.getItem(walletInterstitialConfig.dismissKey);
		setInterstitialDismissed(dismissed === 'true');
	}, [walletInterstitialConfig?.dismissKey]);

	if (
		navigator.userAgent !== 'Playwright' &&
		walletInterstitialConfig?.enabled &&
		!interstitialDismissed
	) {
		return (
			<Interstitial
				{...walletInterstitialConfig}
				onClose={() => {
					setInterstitialDismissed(true);
				}}
			/>
		);
	}
	const accountHasSui = coinBalances?.some(({ coinType }) => coinType === SUI_TYPE_ARG);

	if (!activeAccountAddress) {
		return null;
	}
	return (
		<>
			{isMainnet && data?.degraded && (
				<div className="rounded-2xl bg-warning-light border border-solid border-warning-dark/20 text-warning-dark flex items-center py-2 px-3 mb-4">
					<Info12 className="shrink-0" />
					<div className="ml-2">
						<Text variant="pBodySmall" weight="medium">
							We're sorry that the app is running slower than usual. We're working to fix the issue
							and appreciate your patience.
						</Text>
					</div>
				</div>
			)}

			<Loading loading={isFirstTimeLoading}>
				{coinType && <PageTitle title={coinSymbol} back="/tokens" />}

				<div
					className="flex flex-col h-full flex-1 flex-grow items-center gap-8"
					data-testid="coin-page"
				>
					<AccountsList />
					<BuyNLargeHomePanel />
					<UsdcPromoBanner />

					<div className="flex flex-col w-full">
						<PortfolioName
							name={activeAccount.nickname ?? domainName ?? formatAddress(activeAccountAddress)}
						/>
						{activeAccount.isLocked ? null : (
							<>
								<div
									data-testid="coin-balance"
									className={clsx(
										'rounded-2xl py-5 px-4 flex flex-col w-full gap-3 items-center mt-4',
										isDefiWalletEnabled ? 'bg-gradients-graph-cards' : 'bg-hero/5',
									)}
								>
									<div className="flex flex-col gap-1 items-center">
										<CoinBalance amount={tokenBalance} type={activeCoinType} />
									</div>

									{!accountHasSui ? (
										<div className="flex flex-col gap-5">
											<div className="flex flex-col flex-nowrap justify-center items-center text-center px-2.5">
												<Text variant="pBodySmall" color="gray-80" weight="normal">
													{isMainnet
														? 'Buy SUI to get started'
														: 'To send transactions on the Sui network, you need SUI in your wallet.'}
												</Text>
											</div>
											<FaucetRequestButton />
										</div>
									) : null}
									{isError ? (
										<Alert>
											<div>
												<strong>Error updating balance</strong>
											</div>
										</Alert>
									) : null}
									<div className="grid grid-cols-3 gap-3 w-full">
										{isMainnet ? (
											<LargeButton
												spacing="sm"
												className={
													!accountHasSui && isMainnet
														? 'col-span-3 !bg-sui-primaryBlue2023 !text-white'
														: ''
												}
												primary={!accountHasSui}
												center
												to="/onramp"
												disabled={(coinType && coinType !== SUI_TYPE_ARG) || !providers?.length}
											>
												Buy
											</LargeButton>
										) : null}

										<LargeButton
											center
											data-testid="send-coin-button"
											to={`/send${
												coinBalance?.coinType
													? `?${new URLSearchParams({
															type: coinBalance.coinType,
														}).toString()}`
													: ''
											}`}
											disabled={!tokenBalance}
										>
											Send
										</LargeButton>

										<LargeButton
											center
											disabled={!isDefiWalletEnabled || !tokenBalance}
											to={`/swap${
												coinBalance?.coinType
													? `?${new URLSearchParams({
															type: coinBalance.coinType,
														}).toString()}`
													: ''
											}`}
											onClick={() => {
												if (!coinBalance) {
													return;
												}

												ampli.clickedSwapCoin({
													coinType: coinBalance.coinType,
													totalBalance: Number(formatted),
													sourceFlow: 'LargeButton-TokenDetails',
												});
											}}
										>
											Swap
										</LargeButton>
										{!accountHasSui && (
											<LargeButton disabled to="/stake" center>
												Stake
											</LargeButton>
										)}
									</div>

									<div className="w-full">
										{accountHasSui || delegatedStake?.length ? (
											<TokenIconLink
												disabled={!tokenBalance}
												accountAddress={activeAccountAddress}
											/>
										) : null}
									</div>
								</div>
							</>
						)}
					</div>
					{activeAccount.isLocked ? (
						<UnlockAccountButton account={activeAccount} />
					) : (
						<MyTokens
							coinBalances={coinBalances ?? []}
							isLoading={coinBalancesLoading}
							isFetched={coinBalancesFetched}
						/>
					)}
				</div>
			</Loading>
		</>
	);
}

export default TokenDetails;
