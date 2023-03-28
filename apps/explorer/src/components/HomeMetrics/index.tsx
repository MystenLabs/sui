// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAmount, roundFloat, useRpcClient } from '@mysten/core';
import { Connection, JsonRpcProvider } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { Network } from '../../utils/api/DefaultRpcClient';
import { MetricGroup } from './MetricGroup';

import { useNetwork } from '~/context';
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

const HOME_REFETCH_INTERVAL = 5 * 1000;

export function HomeMetrics() {
    const [network] = useNetwork();
    const rpc = useRpcClient();

    // TODO: Use enhanced RPC locally by default
    const enhancedRpc = useMemo(() => {
        if (network === Network.LOCAL) {
            return new JsonRpcProvider(
                new Connection({ fullnode: 'http://localhost:9124' })
            );
        }

        return rpc;
    }, [network, rpc]);

    const { data: gasData } = useQuery(['home', 'reference-gas-price'], () =>
        rpc.getReferenceGasPrice()
    );

    const { data: transactionCount } = useQuery(
        ['home', 'transaction-count'],
        () => rpc.getTotalTransactionBlocks(),
        { refetchInterval: HOME_REFETCH_INTERVAL }
    );

    const { data: networkMetrics } = useQuery(
        ['home', 'metrics'],
        () => enhancedRpc.getNetworkMetrics(),
        { refetchInterval: HOME_REFETCH_INTERVAL }
    );

    return (
        <Card spacing="lg">
            <Heading variant="heading4/semibold" color="steel-darker">
                Sui Network Stats
            </Heading>

            <div className="mt-8 space-y-7">
                <MetricGroup label="Current">
                    <StatsWrapper label="TPS" tooltip="Transactions per second">
                        {networkMetrics?.currentTps
                            ? roundFloat(networkMetrics.currentTps, 2)
                            : null}
                    </StatsWrapper>
                    <StatsWrapper label="Gas Price" tooltip="Current gas price">
                        {gasData ? `${gasData} MIST` : null}
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
                    <StatsWrapper
                        label="Packages"
                        tooltip="Total packages counter"
                    >
                        {formatAmount(networkMetrics?.totalPackages)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Objects"
                        tooltip="Total objects counter"
                    >
                        {formatAmount(networkMetrics?.totalObjects)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Transaction Blocks"
                        tooltip="Total transaction blocks counter"
                    >
                        {formatAmount(transactionCount)}
                    </StatsWrapper>
                    <StatsWrapper
                        label="Addresses"
                        tooltip="Total addresses counter"
                    >
                        {formatAmount(networkMetrics?.totalAddresses)}
                    </StatsWrapper>
                </MetricGroup>
            </div>
        </Card>
    );
}
