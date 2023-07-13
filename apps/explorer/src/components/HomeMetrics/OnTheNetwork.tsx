// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, formatBalance, useGetReferenceGasPrice } from '@mysten/core';

import { FormattedStatsAmount, StatsWrapper } from './FormattedStatsAmount';
import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

export function OnTheNetwork() {
	const { data: networkMetrics } = useGetNetworkMetrics();
	const { data: referenceGasPrice } = useGetReferenceGasPrice();
	const gasPriceFormatted =
		typeof referenceGasPrice === 'bigint'
			? formatBalance(referenceGasPrice, 0, CoinFormat.FULL)
			: null;
	return (
		<Card bg="white" spacing="lg" height="full">
			<div className="flex flex-col gap-5">
				<Heading variant="heading4/semibold" color="steel-darker">
					Network Activity
				</Heading>
				<div className="flex gap-6">
					<FormattedStatsAmount
						label="TPS now"
						amount={networkMetrics?.currentTps ? Math.floor(networkMetrics.currentTps) : undefined}
						size="sm"
					/>
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
						postfix={gasPriceFormatted !== null ? 'MIST' : null}
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
			</div>
		</Card>
	);
}
