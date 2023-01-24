// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { MetricGroup } from './MetricGroup';

import { useAppsBackend } from '~/hooks/useAppsBackend';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats } from '~/ui/Stats';
import { Network } from '~/utils/api/rpcSetting';
import { useNetwork } from '~/context';

const numberFormatter = new Intl.NumberFormat(undefined);

interface CountsResponse {
    addresses: number;
    objects: number;
    packages: number;
    transactions: number;
}

interface TPSResponse {
    tps: number;
}

function roundFloat(number: number, decimals: number) {
    return parseFloat(number.toFixed(decimals));
}

function formatStat(value?: number) {
    return typeof value === 'number'
        ? numberFormatter.format(value)
        : value ?? '--';
}

const SUPPORTED_NETWORKS: string[] = [Network.LOCAL, Network.TESTNET];

export function HomeMetrics() {
    const [network] = useNetwork();
    const indexerSupported = SUPPORTED_NETWORKS.includes(network);

    const request = useAppsBackend();
    const { data: systemData } = useGetSystemObject();

    const { data: countsData } = useQuery(
        ['home', 'counts'],
        () => request<CountsResponse>('counts', { network }),
        { enabled: indexerSupported }
    );

    const { data: tpsData } = useQuery(
        ['home', 'tps'],
        () => request<TPSResponse>('tps', { network }),
        { enabled: indexerSupported }
    );

    if (!indexerSupported) return null;

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
                        {systemData?.reference_gas_price}
                    </Stats>
                    <Stats label="Epoch" tooltip="The current epoch">
                        {systemData?.epoch}
                    </Stats>
                </MetricGroup>

                <MetricGroup label="Total">
                    <Stats
                        label="Packages"
                        tooltip="Total packages counter (updates every one min)"
                    >
                        {formatStat(countsData?.packages)}
                    </Stats>
                    <Stats
                        label="Objects"
                        tooltip="Total objects counter (updates every one min)"
                    >
                        {formatStat(countsData?.objects)}
                    </Stats>
                    <Stats
                        label="Transactions"
                        tooltip="Total transactions counter (updates every one min)"
                    >
                        {formatStat(countsData?.transactions)}
                    </Stats>
                    <Stats
                        label="Addresses"
                        tooltip="Total addresses counter (updates every one min)"
                    >
                        {formatStat(countsData?.addresses)}
                    </Stats>
                </MetricGroup>
            </div>
        </Card>
    );
}
