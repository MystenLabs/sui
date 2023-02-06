// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useQuery } from '@tanstack/react-query';

import { MetricGroup } from './MetricGroup';

import { useNetwork } from '~/context';
import { useAppsBackend } from '~/hooks/useAppsBackend';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { useRpc } from '~/hooks/useRpc';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats, type StatsProps } from '~/ui/Stats';
import { formatAmount } from '~/utils/formatAmount';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

interface CountsResponse {
    addresses: number;
    objects: number;
    packages: number;
    transactions: number;
}

interface TPSCheckpointResponse {
    tps: number;
    checkpoint: string;
}

function roundFloat(number: number, decimals: number) {
    return parseFloat(number.toFixed(decimals));
}

// Simple wrapper around stats to avoid text wrapping:
function StatsWrapper(props: StatsProps) {
    return (
        <div className="flex-shrink-0">
            <Stats {...props} />
        </div>
    );
}

export function HomeMetrics() {
    const [network] = useNetwork();
    const enabled = useFeature(GROWTHBOOK_FEATURES.EXPLORER_METRICS).on;

    const request = useAppsBackend();
    const { data: systemData } = useGetSystemObject();

    const rpc = useRpc();
    const { data: gasData } = useQuery(['reference-gas-price'], () =>
        rpc.getReferenceGasPrice()
    );

    const { data: countsData } = useQuery(
        ['home', 'counts'],
        () => request<CountsResponse>('counts', { network }),
        { enabled, refetchInterval: 60 * 1000 }
    );

    const { data: tpsData } = useQuery(
        ['home', 'tps-checkpoints'],
        () => request<TPSCheckpointResponse>('tps-checkpoints', { network }),
        { enabled, refetchInterval: 10 * 1000 }
    );

    if (!enabled) return null;

    return (
        <Card spacing="lg">
            <Heading variant="heading4/semibold" color="steel-darker">
                Sui Network Stats
            </Heading>

            <div className="mt-8 space-y-7">
                <MetricGroup label="Current">
                    <StatsWrapper label="TPS" tooltip="Transactions per second">
                        {tpsData?.tps ? roundFloat(tpsData.tps, 2) : null}
                    </StatsWrapper>
                    <StatsWrapper label="Gas Price" tooltip="Current gas price">
                        {gasData ? `${gasData} MIST` : null}
                    </StatsWrapper>
                    <StatsWrapper label="Epoch" tooltip="The current epoch">
                        {systemData?.epoch}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Checkpoint"
                        tooltip="The current checkpoint (updates every one min)"
                    >
                        {tpsData?.checkpoint}
                    </StatsWrapper>
                </MetricGroup>

                <MetricGroup label="Total">
                    <StatsWrapper
                        label="Packages"
                        tooltip="Total packages counter (updates every one min)"
                    >
                        {formatAmount(countsData?.packages)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Objects"
                        tooltip="Total objects counter (updates every one min)"
                    >
                        {formatAmount(countsData?.objects)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Transactions"
                        tooltip="Total transactions counter (updates every one min)"
                    >
                        {formatAmount(countsData?.transactions)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Addresses"
                        tooltip="Total addresses counter (updates every one min)"
                    >
                        {formatAmount(countsData?.addresses)}
                    </StatsWrapper>
                </MetricGroup>
            </div>
        </Card>
    );
}
