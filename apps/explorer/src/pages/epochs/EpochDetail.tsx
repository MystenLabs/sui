// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature, useGrowthBook } from '@growthbook/growthbook-react';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { Navigate, useParams } from 'react-router-dom';

import { CheckpointsTable } from '../checkpoints/CheckpointsTable';
import { validatorsTableData } from '../validators/Validators';
import { EpochProgress } from './stats/EpochProgress';
import { EpochStats } from './stats/EpochStats';
import { ValidatorStatus } from './stats/ValidatorStatus';

import { SuiAmount } from '~/components/transactions/TxCardUtils';
import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { Banner } from '~/ui/Banner';
import { Card } from '~/ui/Card';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Stats } from '~/ui/Stats';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

function EpochDetail() {
    const { id } = useParams();
    const enhancedRpc = useEnhancedRpcClient();
    const { data, isLoading, isError } = useQuery(['epoch', id], async () =>
        enhancedRpc.getEpochs({
            // todo: endpoint returns no data for epoch 0
            cursor: id === '0' ? undefined : +id! - 1,
            limit: 1,
        })
    );

    const [epochData] = data?.data ?? [];
    const isCurrentEpoch = !epochData?.endOfEpochInfo;

    const validatorsTable = useMemo(() => {
        if (!epochData?.validators) return null;
        // todo: enrich this historical validator data when we have
        // at-risk / pending validators for historical epochs
        return validatorsTableData(epochData.validators, [], [], null);
    }, [epochData]);

    if (isError || !epochData)
        return (
            <Banner variant="error" fullWidth>
                {`There was an issue retrieving data for epoch ${id}.`}
            </Banner>
        );

    if (isLoading) return <LoadingSpinner />;

    return (
        <div className="flex flex-col space-y-16">
            <div className="grid grid-flow-row gap-4 sm:gap-2 md:flex md:gap-6">
                <EpochProgress
                    epoch={epochData?.epoch}
                    inProgress={isCurrentEpoch}
                    start={epochData?.epochStartTimestamp}
                    end={epochData?.endOfEpochInfo?.epochEndTimestamp}
                />

                <EpochStats label="Activity">
                    <Stats label="Gas Revenue" tooltip="Gas Revenue">
                        <SuiAmount
                            amount={epochData.endOfEpochInfo?.totalGasFees}
                        />
                    </Stats>
                    <Stats label="Storage Revenue" tooltip="Storage Revenue">
                        <SuiAmount
                            amount={epochData?.endOfEpochInfo?.storageCharge}
                        />
                    </Stats>
                    <Stats label="Stake Rewards" tooltip="Stake Rewards">
                        <SuiAmount
                            amount={
                                epochData?.endOfEpochInfo
                                    ?.totalStakeRewardsDistributed
                            }
                        />
                    </Stats>
                </EpochStats>

                <EpochStats label="Rewards">
                    <Stats label="Stake Subsidies" tooltip="Stake Subsidies">
                        <SuiAmount
                            amount={
                                epochData?.endOfEpochInfo?.stakeSubsidyAmount
                            }
                        />
                    </Stats>
                    <Stats label="Total Rewards" tooltip="Total Rewards">
                        <SuiAmount
                            amount={
                                epochData?.endOfEpochInfo
                                    ?.totalStakeRewardsDistributed
                            }
                        />
                    </Stats>

                    <Stats
                        label="Storage Fund Earnings"
                        tooltip="Storage Fund Earnings"
                    >
                        <SuiAmount
                            amount={
                                epochData?.endOfEpochInfo
                                    ?.leftoverStorageFundInflow
                            }
                        />
                    </Stats>
                </EpochStats>
                {isCurrentEpoch ? (
                    <Card spacing="lg">
                        <ValidatorStatus />
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
                            initialCursor={epochData?.endOfEpochInfo?.lastCheckpointId.toString()}
                            maxCursor={epochData?.firstCheckpointId.toString()}
                            initialLimit={20}
                        />
                    </TabPanel>
                    <TabPanel>
                        {validatorsTable ? (
                            <TableCard
                                data={validatorsTable.data}
                                columns={validatorsTable.columns}
                                sortTable
                                defaultSorting={[{ id: 'stake', desc: false }]}
                            />
                        ) : null}
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}

export default function EpochDetailFeatureFlagged() {
    const gb = useGrowthBook();
    const { on: epochsEnabled } = useFeature(
        GROWTHBOOK_FEATURES.EPOCHS_CHECKPOINTS
    );
    if (!gb?.ready) return <LoadingSpinner />;
    if (epochsEnabled) return <EpochDetail />;
    return <Navigate to="/" />;
}
