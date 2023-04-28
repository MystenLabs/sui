// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetTotalTransactionBlocks } from '@mysten/core';
import { Svg3D32, Nft232, Wallet32, Staking32 } from '@mysten/icons';

import { FormattedStatsAmount } from './FormattedStatsAmount';

import { useGetNetworkMetrics } from '~/hooks/useGetNetworkMetrics';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

export function OnTheNetwork() {
    const { data: networkMetrics } = useGetNetworkMetrics();
    const { data: transactionCount } = useGetTotalTransactionBlocks();
    return (
        <Card bg="lightBlue" spacing="lg">
            <Heading color="steel-darker" variant="heading4/semibold">
                On the Network
            </Heading>
            <div className="-mb-3 -mr-8 mt-8 flex gap-8 overflow-x-auto pb-3">
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
                    <div className="flex flex-shrink-0 gap-1 pr-2">
                        <Staking32 className="h-8 w-8 text-steel-dark" />
                        <FormattedStatsAmount
                            label="Transaction Blocks"
                            tooltip="Total transaction blocks counter"
                            amount={transactionCount}
                            size="sm"
                        />
                    </div>
                </div>
            </div>
        </Card>
    );
}
