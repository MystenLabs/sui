// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StatsWrapper } from './FormattedStatsAmount';
import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';

export function Checkpoint() {
	const { data, isLoading } = useGetNetworkMetrics();

	return (
		<StatsWrapper
			label="Checkpoint"
			tooltip="The current checkpoint"
			unavailable={isLoading}
			size="sm"
			orientation="horizontal"
		>
			{data?.currentCheckpoint ? BigInt(data?.currentCheckpoint).toLocaleString() : null}
		</StatsWrapper>
	);
}
