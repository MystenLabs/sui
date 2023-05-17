// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StatsWrapper } from './FormattedStatsAmount';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

export function NetworkTPS() {
    const { data: networkMetrics } = useGetNetworkMetrics();

    return (
        <Card bg="lightBlue" spacing="lg">
            <Heading color="steel-darker" variant="heading4/semibold">
                Network TPS
            </Heading>
            <div className="mt-8 flex gap-8">
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
        </Card>
    );
}
