// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useGetAllBalances } from '_app/hooks/useGetAllBalances';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { useUsdcPromo } from '_pages/home/usdc-promo/useUsdcPromo';
import { useCoinMetadata } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { type CoinBalance } from '@mysten/sui/client';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

function BannerImage({ balance }: { balance: CoinBalance }) {
	const navigate = useNavigate();
	const { promoBannerImage } = useUsdcPromo();
	const { data: metadata } = useCoinMetadata(balance.coinType);

	const maxBalance = useMemo(() => {
		const decimals = metadata?.decimals ?? 0;
		return new BigNumber(balance?.totalBalance || 0)
			.shiftedBy(-decimals)
			.decimalPlaces(decimals)
			.toString();
	}, [balance, metadata]);

	return (
		<img
			role="button"
			className="w-full cursor-pointer"
			alt="USDC Promo"
			src={promoBannerImage}
			onClick={() => {
				navigate(
					`/usdc-promo?${new URLSearchParams({
						type: balance.coinType,
						presetAmount: maxBalance,
					})}`,
				);
			}}
		/>
	);
}

export function UsdcPromoBanner() {
	const activeAccountAddress = useActiveAddress();
	const { enabled, wrappedUsdcList } = useUsdcPromo();

	const { data: coinBalances } = useGetAllBalances(activeAccountAddress || '');

	const usdcInUsersBalance = useMemo(() => {
		if (!coinBalances) {
			return [];
		}
		return coinBalances.filter(
			(coin) => wrappedUsdcList.includes(coin.coinType) && Number(coin.totalBalance) > 0,
		);
	}, [coinBalances, wrappedUsdcList]);

	const firstUsdcInUsersBalance = usdcInUsersBalance[0];

	if (!enabled || !firstUsdcInUsersBalance) {
		return null;
	}

	return <BannerImage balance={firstUsdcInUsersBalance} />;
}
