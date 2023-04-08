// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmountParts, roundFloat, useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import { MetricGroup } from './MetricGroup';

import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats, type StatsProps } from '~/ui/Stats';

// Simple wrapper around stats to avoid text wrapping:
function StatsWrapper(props: StatsProps) {
    return (
        <div className="flex-shrink-0">
            <Stats {...props} />
        </div>
    );
}

function FormattedStatsAmount({
    amount,
    ...props
}: Omit<StatsProps, 'children'> & { amount?: string | number | bigint }) {
    const [formattedAmount, postfix] = formatAmountParts(amount);

    return (
        <StatsWrapper {...props} postfix={postfix}>
            {formattedAmount}
        </StatsWrapper>
    );
}

// const HOME_REFETCH_INTERVAL = 5 * 1000;

export function HomeMetrics() {
    const rpc = useRpcClient();

    // todo: remove this hook when we enable enhanced rpc client by default
    const enhancedRpc = useEnhancedRpcClient();

    const { data: gasData } = useQuery(['home', 'reference-gas-price'], () =>
        rpc.getReferenceGasPrice()
    );

    const { data: transactionCount } = useQuery(
        ['home', 'transaction-count'],
        () => rpc.getTotalTransactionBlocks(),
        { cacheTime: 24 * 60 * 60 * 1000, staleTime: Infinity, retry: false }
    );

    const { data: networkMetrics } = useQuery(
        ['home', 'metrics'],
        () => enhancedRpc.getNetworkMetrics(),
        { cacheTime: 24 * 60 * 60 * 1000, staleTime: Infinity, retry: false }
    );

    return (
        <Card spacing="lg">
            <Heading variant="heading4/semibold" color="steel-darker">
                Sui Network Stats
            </Heading>

            <div className="mt-8 space-y-7">
                <MetricGroup label="Current">
                    <StatsWrapper
                        label="TPS Now / Peak 30D"
                        tooltip="Peak TPS in the past 30 days excluding this epoch"
                        postfix={`/ ${
                            networkMetrics?.tps30Days
                                ? roundFloat(networkMetrics.tps30Days, 2)
                                : '--'
                        }`}
                    >
                        {networkMetrics?.currentTps
                            ? roundFloat(networkMetrics.currentTps, 2)
                            : '--'}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Gas Price"
                        tooltip="Current gas price"
                        postfix="MIST"
                    >
                        {gasData ? String(gasData) : null}
                    </StatsWrapper>
                    <StatsWrapper label="Epoch" tooltip="The current epoch">
                        {networkMetrics?.currentEpoch}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Checkpoint"
                        tooltip="The current checkpoint"
                    >
                        {networkMetrics?.currentCheckpoint}
                    </StatsWrapper>
                </MetricGroup>

                <MetricGroup label="Total">
                    <FormattedStatsAmount
                        label="Packages"
                        tooltip="Total packages counter"
                        amount={networkMetrics?.totalPackages}
                    />
                    <FormattedStatsAmount
                        label="Objects"
                        tooltip="Total objects counter"
                        amount={networkMetrics?.totalObjects}
                    />
                    <FormattedStatsAmount
                        label="Transaction Blocks"
                        tooltip="Total transaction blocks counter"
                        amount={transactionCount}
                    />
                    <FormattedStatsAmount
                        label="Addresses"
                        tooltip="Addresses that have participated in at least one transaction since network genesis"
                        amount={networkMetrics?.totalAddresses}
                    />
                </MetricGroup>
            </div>
        </Card>
    );
}
