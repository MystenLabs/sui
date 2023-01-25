// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useQuery } from '@tanstack/react-query';

import { MetricGroup } from './MetricGroup';

import { useNetwork } from '~/context';
import { useAppsBackend } from '~/hooks/useAppsBackend';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats } from '~/ui/Stats';
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

export function HomeMetrics() {
    const [network] = useNetwork();
    const enabled = useFeature(GROWTHBOOK_FEATURES.EXPLORER_METRICS).on;

    const request = useAppsBackend();
    const { data: systemData } = useGetSystemObject();

    const { data: countsData } = useQuery(
        ['home', 'counts'],
        () => request<CountsResponse>('counts', { network }),
        { enabled }
    );

    const { data: tpsData } = useQuery(
        ['home', 'tps-checkpoints'],
        () => request<TPSCheckpointResponse>('tps-checkpoints', { network }),
        { enabled }
    );

    if (!enabled) return null;

    return (
        <Card spacing="lg">
            <Heading variant="heading4/semibold" color="steel-darker">
                Sui Network Stats
            </Heading>

            <div className="mt-8 space-y-7">
                <MetricGroup label="Current">
                    <Stats label="TPS" tooltip="Transactions per second">
                        {tpsData?.tps ? roundFloat(tpsData.tps, 2) : null}
                    </Stats>
                    <Stats label="Gas Price" tooltip="Current gas price">
                        {systemData?.reference_gas_price
                            ? `${systemData?.reference_gas_price} MIST`
                            : null}
                    </Stats>
                    <Stats label="Epoch" tooltip="The current epoch">
                        {systemData?.epoch}
                    </Stats>
                    <Stats
                        label="Checkpoint"
                        tooltip="The current checkpoint (updates every one min)"
                    >
                        {tpsData?.checkpoint}
                    </Stats>
                </MetricGroup>

                <MetricGroup label="Total">
                    <Stats
                        label="Packages"
                        tooltip="Total packages counter (updates every one min)"
                    >
                        {formatAmount(countsData?.packages)}
                    </Stats>
                    <Stats
                        label="Objects"
                        tooltip="Total objects counter (updates every one min)"
                    >
                        {formatAmount(countsData?.objects)}
                    </Stats>
                    <Stats
                        label="Transactions"
                        tooltip="Total transactions counter (updates every one min)"
                    >
                        {formatAmount(countsData?.transactions)}
                    </Stats>
                    <Stats
                        label="Addresses"
                        tooltip="Total addresses counter (updates every one min)"
                    >
                        {formatAmount(countsData?.addresses)}
                    </Stats>
                </MetricGroup>
            </div>
        </Card>
    );
}
