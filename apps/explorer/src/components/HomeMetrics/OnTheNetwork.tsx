// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, formatBalance, useGetReferenceGasPrice } from '@mysten/core';
import { Heading } from '@mysten/ui';

import { FormattedStatsAmount, StatsWrapper } from './FormattedStatsAmount';
import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';
import { Divider } from '~/ui/Divider';

export function OnTheNetwork() {
	const { data: networkMetrics } = useGetNetworkMetrics();
	const { data: referenceGasPrice } = useGetReferenceGasPrice();
	const gasPriceFormatted =
		typeof referenceGasPrice === 'bigint'
			? formatBalance(referenceGasPrice, 0, CoinFormat.FULL)
			: null;
	return (
		<Card bg="white/80" spacing="lg" height="full">
			<div className="flex flex-col gap-4">
				<Heading variant="heading4/semibold" color="steel-darker">
					Network Activity
				</Heading>
				<div className="flex gap-6">
					<FormattedStatsAmount
						label="TPS now"
						amount={networkMetrics?.currentTps ? Math.floor(networkMetrics.currentTps) : undefined}
						size="md"
					/>
					<FormattedStatsAmount
						label="Peak 30d TPS"
						tooltip="Peak TPS in the past 30 days excluding this epoch"
						amount={networkMetrics?.tps30Days ? Math.floor(networkMetrics?.tps30Days) : undefined}
						size="md"
					/>
				</div>
				<Divider color="hero/10" />

				<StatsWrapper
					orientation="horizontal"
					label="Reference Gas Price"
					tooltip="The reference gas price of the current epoch"
					postfix={gasPriceFormatted !== null ? 'MIST' : null}
					size="sm"
				>
					{gasPriceFormatted}
				</StatsWrapper>

				<Divider color="hero/10" />

				<div className="flex flex-1 flex-col gap-2">
					<FormattedStatsAmount
						orientation="horizontal"
						label="Total Packages"
						amount={networkMetrics?.totalPackages}
						size="sm"
					/>
					<FormattedStatsAmount
						orientation="horizontal"
						label="Objects"
						amount={networkMetrics?.totalObjects}
						size="sm"
					/>
				</div>
			</div>
		</Card>
	);
}
