// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetAllBalances } from '_app/hooks/useGetAllBalances';
import { Text } from '_app/shared/text';
import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import { useActiveAddress } from '_hooks';
import { useUsdcPromo } from '_pages/home/usdc-promo/useUsdcPromo';
import { ampli } from '_shared/analytics/ampli';
import { useCoinMetadata } from '@mysten/core';
import { type CoinBalance } from '@mysten/sui/client';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

function useUsdcInUserBalance() {
	const activeAccountAddress = useActiveAddress();
	const { wrappedUsdcList } = useUsdcPromo();

	const { data: coinBalances } = useGetAllBalances(activeAccountAddress || '');

	return coinBalances
		? coinBalances.filter(
				(coin) => wrappedUsdcList.includes(coin.coinType) && Number(coin.totalBalance) > 0,
			)
		: [];
}

function BannerImage({ balance }: { balance: CoinBalance }) {
	const navigate = useNavigate();
	const { promoBannerBackground, promoBannerText } = useUsdcPromo();
	const { data: metadata } = useCoinMetadata(balance.coinType);
	const usdcInUsersBalance = useUsdcInUserBalance();

	const maxBalance = useMemo(() => {
		const decimals = metadata?.decimals ?? 0;
		return new BigNumber(balance?.totalBalance || 0)
			.shiftedBy(-decimals)
			.decimalPlaces(decimals)
			.toString();
	}, [balance, metadata]);

	return (
		<ButtonOrLink
			className="relative bg-transparent border-none p-0"
			onClick={() => {
				ampli.clickedUsdcPromoBanner({
					wUsdcInAccount: usdcInUsersBalance.map((coin) => coin.coinType),
				});
				navigate(
					`/usdc-promo?${new URLSearchParams({
						type: balance.coinType,
						presetAmount: maxBalance,
					})}`,
				);
			}}
		>
			<img className="w-full cursor-pointer h-16" alt="USDC Promo" src={promoBannerBackground} />
			<div className="absolute top-1/2 -translate-y-1/2 w-full flex flex-row gap-4 px-3">
				<img
					alt="USDC"
					src="https://fe-assets.mystenlabs.com/wallet_next/usdc_icon.png"
					className="h-8 w-8"
				/>
				<Text variant="bodySmall" weight="medium" color="white" className="text-left">
					{promoBannerText}
				</Text>
			</div>
		</ButtonOrLink>
	);
}

export function UsdcPromoBanner() {
	const { enabled } = useUsdcPromo();
	const usdcInUsersBalance = useUsdcInUserBalance();

	const firstUsdcInUsersBalance = usdcInUsersBalance[0];

	if (!enabled || !firstUsdcInUsersBalance) {
		return null;
	}

	return <BannerImage balance={firstUsdcInUsersBalance} />;
}
