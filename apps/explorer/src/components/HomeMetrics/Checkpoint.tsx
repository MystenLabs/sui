// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StatsWrapper } from './FormattedStatsAmount';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';

export function Checkpoint() {
    const { data, isLoading } = useGetNetworkMetrics();

    return (
        <Card height="full" spacing="lg">
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
        </Card>
    );
}
