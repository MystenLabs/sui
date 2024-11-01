// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import PageTitle from '_app/shared/PageTitle';
import { Text } from '_app/shared/text';
import { useUsdcPromo } from '_pages/home/usdc-promo/useUsdcPromo';
import { USDC_TYPE_ARG } from '_pages/swap/utils';
import { ampli } from '_shared/analytics/ampli';
import { useNavigate, useSearchParams } from 'react-router-dom';

export function UsdcPromo() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const fromCoinType = searchParams.get('type');
	const presetAmount = searchParams.get('presetAmount');
	const { promoBannerSheetTitle, promoBannerSheetContent, ctaLabel } = useUsdcPromo();

	return (
		<div className="flex flex-col items-center gap-6">
			<PageTitle back />
			<img
				src="https://fe-assets.mystenlabs.com/wallet_next/usdc_icon.png"
				alt="USDC"
				className="h-16 w-16"
			/>
			<div className="flex flex-col gap-2 text-center">
				<Heading as="h1" variant="heading2" weight="semibold">
					{promoBannerSheetTitle}
				</Heading>
				<Text variant="pBody" weight="medium" color="gray-90">
					{promoBannerSheetContent}
				</Text>
			</div>
			<Button
				text={ctaLabel}
				onClick={() => {
					ampli.clickedSwapCoin({
						sourceFlow: 'UsdcPromoBanner',
						coinType: fromCoinType || '',
						totalBalance: Number(presetAmount || 0),
					});

					navigate(
						`/swap?${new URLSearchParams({
							type: fromCoinType || '',
							toType: USDC_TYPE_ARG,
							presetAmount: presetAmount || '',
						})}`,
					);
				}}
			/>
		</div>
	);
}
