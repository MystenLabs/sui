// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getRefGasPrice, useFormatCoin, useGetSystemState } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { CheckpointsTable } from '../checkpoints/CheckpointsTable';
import { validatorsTableData } from '../validators/Validators';
import { EpochProgress } from './stats/EpochProgress';
import { EpochStats } from './stats/EpochStats';
import { ValidatorStatus } from './stats/ValidatorStatus';

import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { Banner } from '~/ui/Banner';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Stats, type StatsProps } from '~/ui/Stats';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { getEpochStorageFundFlow } from '~/utils/getStorageFundFlow';
import { Text } from '~/ui/Text';

function SuiStats({
    amount,
    ...props
}: Omit<StatsProps, 'children'> & {
    amount: bigint | number | string | undefined | null;
}) {
    const [formattedAmount, symbol] = useFormatCoin(amount, SUI_TYPE_ARG);

    return (
        <Stats postfix={formattedAmount && symbol} {...props}>
            {formattedAmount || '--'}
        </Stats>
    );
}

export default function EpochDetail() {
    const { id } = useParams();
    const enhancedRpc = useEnhancedRpcClient();
    const { data: systemState } = useGetSystemState();
    const { data, isLoading, isError } = useQuery(['epoch', id], async () =>
        enhancedRpc.getEpochs({
            // todo: endpoint returns no data for epoch 0
            cursor: id === '0' ? undefined : (Number(id!) - 1).toString(),
            limit: 1,
        })
    );

    const nextRefGasPrice = useMemo(
        () => getRefGasPrice(systemState?.activeValidators),
        [systemState?.activeValidators]
    );

    const [epochData] = data?.data ?? [];
    const isCurrentEpoch = useMemo(
        () => systemState?.epoch === epochData?.epoch,
        [systemState, epochData]
    );

    const validatorsTable = useMemo(() => {
        if (!epochData?.validators) return null;
        // todo: enrich this historical validator data when we have
        // at-risk / pending validators for historical epochs
        return validatorsTableData(
            [...epochData.validators].sort(() => 0.5 - Math.random()),
            [],
            [],
            null
        );
    }, [epochData]);

    if (isLoading) return <LoadingSpinner />;

    if (isError || !epochData)
        return (
            <Banner variant="error" fullWidth>
                {`There was an issue retrieving data for epoch ${id}.`}
            </Banner>
        );

    const { fundInflow, fundOutflow, netInflow } = getEpochStorageFundFlow(
        epochData.endOfEpochInfo
    );

    return (
        <div className="flex flex-col space-y-16">
            <div className="grid grid-flow-row gap-4 sm:gap-2 md:flex md:gap-6">
                <div className="flex min-w-[136px] max-w-[240px]">
                    <EpochProgress
                        epoch={epochData.epoch}
                        inProgress={isCurrentEpoch}
                        start={Number(epochData.epochStartTimestamp)}
                        end={Number(
                            epochData.endOfEpochInfo?.epochEndTimestamp ?? 0
                        )}
                    />
                </div>

                <EpochStats label="Rewards">
                    <SuiStats
                        label="Total Stake"
                        tooltip=""
                        amount={epochData.endOfEpochInfo?.totalStake}
                    />
                    <SuiStats
                        label="Stake Subsidies"
                        amount={epochData.endOfEpochInfo?.stakeSubsidyAmount}
                    />
                    <SuiStats
                        label="Stake Rewards"
                        amount={
                            epochData.endOfEpochInfo
                                ?.totalStakeRewardsDistributed
                        }
                    />
                    <SuiStats
                        label="Gas Fees"
                        amount={epochData.endOfEpochInfo?.totalGasFees}
                    />
                </EpochStats>

                <EpochStats label="Storage Fund Balance">
                    <SuiStats
                        label="Fund Size"
                        amount={epochData.endOfEpochInfo?.storageFundBalance}
                    />
                    <SuiStats label="Net Inflow" amount={netInflow} />
                    <SuiStats label="Fund Inflow" amount={fundInflow} />
                    <SuiStats label="Fund Outflow" amount={fundOutflow} />
                </EpochStats>

                {isCurrentEpoch ? (
                    <Card spacing="lg" bg="white">
                        <ValidatorStatus />

                        <div className="mt-8 flex items-center justify-between rounded-lg border border-solid border-steel px-3 py-2">
                            <div>
                                <Text
                                    variant="pSubtitle/semibold"
                                    color="steel-darker"
                                >
                                    Estimated Next Epoch
                                </Text>
                                <Text
                                    variant="pSubtitle/semibold"
                                    color="steel-darker"
                                >
                                    Reference Gas Price
                                </Text>
                            </div>
                            <div>
                                <Heading
                                    variant="heading4/semibold"
                                    color="steel-darker"
                                >
                                    {nextRefGasPrice}
                                </Heading>
                                <Text
                                    variant="pBody/medium"
                                    color="steel-darker"
                                >
                                    MIST
                                </Text>
                            </div>
                        </div>
                    </Card>
                ) : null}
            </div>

            <TabGroup size="lg">
                <TabList>
                    <Tab>Checkpoints</Tab>
                    <Tab>Participating Validators</Tab>
                </TabList>
                <TabPanels className="mt-4">
                    <TabPanel>
                        <CheckpointsTable
                            initialCursor={
                                epochData.endOfEpochInfo?.lastCheckpointId
                            }
                            maxCursor={epochData.firstCheckpointId}
                            initialLimit={20}
                        />
                    </TabPanel>
                    <TabPanel>
                        {validatorsTable ? (
                            <TableCard
                                data={validatorsTable.data}
                                columns={validatorsTable.columns}
                                sortTable
                            />
                        ) : null}
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}
