// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetTotalTransactionBlocks } from '@mysten/core';
import { Svg3D32, Nft232, Wallet32, Staking32 } from '@mysten/icons';

import { FormattedStatsAmount } from './FormattedStatsAmount';
import { NetworkStats } from './NetworkStats';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';

export function OnTheNetwork() {
    const { data: networkMetrics } = useGetNetworkMetrics();
    const { data: transactionCount } = useGetTotalTransactionBlocks();
    return (
        <NetworkStats label="On the Network" bg="lightBlue" spacing="none">
            <div className="flex gap-8">
                <div className="flex flex-shrink-0 gap-1">
                    <Svg3D32 className="h-8 w-8 font-normal text-steel-dark" />
                    <FormattedStatsAmount
                        label="Packages"
                        tooltip="Total packages counter"
                        amount={networkMetrics?.totalPackages}
                        size="sm"
                    />
                </div>
                <div className="flex flex-shrink-0 gap-1">
                    <Nft232 className="h-8 w-8 text-steel-dark" />
                    <FormattedStatsAmount
                        label="Objects"
                        tooltip="Total objects counter"
                        amount={networkMetrics?.totalObjects}
                        size="sm"
                    />
                </div>
                <div className="flex flex-shrink-0 gap-1">
                    <Wallet32 className="h-8 w-8 text-steel-dark" />
                    <FormattedStatsAmount
                        label="Addresses"
                        tooltip="Addresses that have participated in at least one transaction since network genesis"
                        amount={networkMetrics?.totalAddresses}
                        size="sm"
                    />
                </div>
                <div className="flex flex-shrink-0 gap-1">
                    <Staking32 className="h-8 w-8 text-steel-dark" />
                    <FormattedStatsAmount
                        label="Transaction Blocks"
                        tooltip="Total transaction blocks counter"
                        amount={transactionCount}
                        size="sm"
                    />
                </div>
            </div>
        </NetworkStats>
    );
}
