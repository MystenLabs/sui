// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useGetSystemState,
    useRpcClient,
    useGetTotalTransactionBlocks,
} from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import { FormattedStatsAmount, StatsWrapper } from './FormattedStatsAmount';
import { MetricGroup } from './MetricGroup';

import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

// const HOME_REFETCH_INTERVAL = 5 * 1000;

export function HomeMetrics() {
    const rpc = useRpcClient();

    // todo: remove this hook when we enable enhanced rpc client by default
    const enhancedRpc = useEnhancedRpcClient();

    const { data: gasData } = useQuery(['home', 'reference-gas-price'], () =>
        rpc.getReferenceGasPrice()
    );

    const { data: systemState } = useGetSystemState();

    const { data: transactionCount } = useGetTotalTransactionBlocks();

    const { data: networkMetrics } = useQuery(
        ['home', 'metrics'],
        () => enhancedRpc.getNetworkMetrics(),
        { cacheTime: 24 * 60 * 60 * 1000, staleTime: Infinity, retry: 5 }
    );

    return (
        <Card spacing="none">
            <div className="pl-8 pt-8">
                <Heading variant="heading4/semibold" color="steel-darker">
                    Sui Network Stats
                </Heading>
            </div>

            <div className="mt-8 space-y-7 pb-8">
                <MetricGroup label="Current">
                    <StatsWrapper
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
                            ? Math.floor(
                                  networkMetrics.currentTps
                              ).toLocaleString()
                            : '--'}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Gas Price"
                        tooltip="Current gas price"
                        postfix="MIST"
                    >
                        {gasData ? gasData.toLocaleString() : null}
                    </StatsWrapper>
                    <StatsWrapper label="Epoch" tooltip="The current epoch">
                        {systemState?.epoch
                            ? BigInt(systemState?.epoch).toLocaleString()
                            : null}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Checkpoint"
                        tooltip="The current checkpoint"
                    >
                        {networkMetrics?.currentCheckpoint
                            ? BigInt(
                                  networkMetrics?.currentCheckpoint
                              ).toLocaleString()
                            : null}
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
                    {/*
                        TODO: enable when indexer is healthy
                        <FormattedStatsAmount
                        label="Addresses"
                        tooltip="Addresses that have participated in at least one transaction since network genesis"
                        amount={networkMetrics?.totalAddresses}
                    /> */}
                </MetricGroup>
            </div>
        </Card>
    );
}
