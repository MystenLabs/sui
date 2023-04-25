// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StatsWrapper } from './FormattedStatsAmount';
import { NetworkStats } from './NetworkStats';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';

export function Checkpoint() {
    const { data, isLoading } = useGetNetworkMetrics();

    return (
        <NetworkStats spacing="none">
            <div className="flex gap-8">
                <StatsWrapper
                    label="Checkpoint"
                    tooltip="The current checkpoint"
                    unavailable={isLoading}
                    size="sm"
                >
                    {data?.currentCheckpoint
                        ? BigInt(data?.currentCheckpoint).toLocaleString()
                        : null}
                </StatsWrapper>
            </div>
        </NetworkStats>
    );
}
