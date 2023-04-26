// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StatsWrapper } from './FormattedStatsAmount';
import { NetworkStats } from './NetworkStats';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';

export function NetworkTPS() {
    const { data: networkMetrics } = useGetNetworkMetrics();

    return (
        <NetworkStats label="Network TPS" bg="lightBlue" spacing="none">
            <div className="flex gap-8">
                <StatsWrapper
                    size="sm"
                    label="TPS Now / Peak 30D"
                    tooltip="Peak TPS in the past 30 days excluding this epoch"
                    postfix={`/ ${
                        networkMetrics?.tps30Days
                            ? Math.floor(
                                  networkMetrics.tps30Days
                              ).toLocaleString()
                            : '--'
                    }`}
                >
                    {networkMetrics?.currentTps
                        ? Math.floor(networkMetrics.currentTps).toLocaleString()
                        : '--'}
                </StatsWrapper>
            </div>
        </NetworkStats>
    );
}
