// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetReferenceGasPrice } from '@mysten/core';
import { Sui } from '@mysten/icons';

import { FormattedStatsAmount, StatsWrapper } from './FormattedStatsAmount';
import { useGasPriceFormat } from '../GasPriceCard/utils';
import { useNetwork } from '~/context';
import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { Network } from '~/utils/api/DefaultRpcClient';

export function OnTheNetwork() {
	const { data: networkMetrics } = useGetNetworkMetrics();
	const { data: referenceGasPrice } = useGetReferenceGasPrice();
	const gasPriceFormatted = useGasPriceFormat(referenceGasPrice || null, 'MIST');
	const { data: tokenData } = useSuiCoinData();
	const { currentPrice } = tokenData || {};
	const formattedPrice = currentPrice
		? currentPrice.toLocaleString('en', {
				style: 'currency',
				currency: 'USD',
		  })
		: '--';
	const [network] = useNetwork();
	const isSuiTokenCardEnabled = network === Network.MAINNET;

	return (
		<Card bg="white" spacing="lg" height="full">
			<div className="flex flex-col gap-5">
				<Heading variant="heading4/semibold" color="steel-darker">
					Network Activity
				</Heading>
				<div className="flex gap-2">
					<FormattedStatsAmount label="TPS now" amount={networkMetrics?.currentTps} size="sm" />
					<FormattedStatsAmount
						label="Peak 30d TPS"
						tooltip="Peak TPS in the past 30 days excluding this epoch"
						amount={networkMetrics?.tps30Days ? Math.floor(networkMetrics?.tps30Days) : undefined}
						size="sm"
					/>
				</div>
				<hr className="flex-1 border-hero/10" />
				<div className="flex flex-1 flex-col gap-2">
					<StatsWrapper
						orientation="horizontal"
						label="Reference Gas Price"
						tooltip="The reference gas price of the current epoch"
						postfix="MIST"
						color="hero-dark"
						size="sm"
					>
						{gasPriceFormatted}
					</StatsWrapper>
					<FormattedStatsAmount
						orientation="horizontal"
						label="Total Packages"
						tooltip="Total packages counter"
						amount={networkMetrics?.totalPackages}
						size="sm"
					/>
					<FormattedStatsAmount
						orientation="horizontal"
						label="Objects"
						tooltip="Total objects counter"
						amount={networkMetrics?.totalObjects}
						size="sm"
					/>
				</div>
				{isSuiTokenCardEnabled ? (
					<>
						<hr className="flex-1 border-hero/10" />
						<div className="flex gap-2">
							<div className="h-5 w-5 rounded-full bg-sui p-1">
								<Sui className="h-full w-full text-white" />
							</div>
							<div className="flex w-full flex-col gap-0.5">
								<Heading variant="heading4/semibold" color="steel-darker">
									1 SUI = {formattedPrice}
								</Heading>
								<Text variant="subtitleSmallExtra/medium" color="steel">
									via CoinGecko
								</Text>
							</div>
						</div>
					</>
				) : null}
			</div>
		</Card>
	);
}
