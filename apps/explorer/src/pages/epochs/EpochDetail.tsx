// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature, useGrowthBook } from '@growthbook/growthbook-react';
import { Navigate } from 'react-router-dom';

import { validatorsTableData } from '../validators/Validators';
import { CheckpointsTable } from './CheckpointsTable';
import { getMockEpochData } from './mocks';
import { EpochStats } from './stats/EpochStats';

import { SuiAmount } from '~/components/transaction-card/TxCardUtils';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { useGetValidatorsEvents } from '~/hooks/useGetValidatorsEvents';
import { EpochProgress } from '~/pages/epochs/stats/EpochProgress';
import { Banner } from '~/ui/Banner';
import { Card } from '~/ui/Card';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { RingChart } from '~/ui/RingChart';
import { Stats } from '~/ui/Stats';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

function EpochDetail() {
    const enabled = useFeature(GROWTHBOOK_FEATURES.EPOCHS_CHECKPOINTS).on;
    const {
        startTimestamp,
        endTimestamp,
        storageSize,
        gasCostSummary,
        totalRewards,
        storageFundEarnings,
        stakeSubsidies,
    } = getMockEpochData();

    const epochQuery = useGetSystemObject();

    const { active, pending, atRisk } = {
        active: epochQuery.data?.validators.active_validators.length,
        pending: epochQuery.data?.validators.pending_validators.contents.size,
        atRisk: epochQuery.data?.validators.pending_removals.length,
    };

    const { data: validatorEvents, isLoading: validatorsEventsLoading } =
        useGetValidatorsEvents({
            limit: epochQuery.data?.validators.active_validators.length || 0,
            order: 'descending',
        });

    if (!enabled) return <Navigate to="/" />;
    if (epochQuery.isError)
        return (
            <Banner variant="error" fullWidth>
                There was an issue retrieving data for the current epoch
            </Banner>
        );

    if (epochQuery.isLoading || validatorsEventsLoading)
        return <LoadingSpinner />;
    if (!epochQuery.data || !validatorEvents) return null;

    const validatorsTable = validatorsTableData(
        epochQuery?.data.validators.active_validators,
        epochQuery?.data.epoch,
        validatorEvents?.data,
        epochQuery?.data.parameters.min_validator_stake
    );

    return (
        <div className="flex flex-col space-y-16">
            <div className="grid grid-cols-1 gap-4 sm:gap-2 md:flex md:gap-6">
                <EpochProgress
                    epoch={epochQuery.data.epoch}
                    inProgress
                    start={startTimestamp!}
                    end={endTimestamp}
                />
                <EpochStats label="Activity">
                    <Stats label="Storage Size" tooltip="Storage Size">
                        {`${storageSize.toFixed(2)} GB`}
                    </Stats>
                    <Stats label="Gas Revenue" tooltip="Gas Revenue">
                        <SuiAmount amount={gasCostSummary?.gasRevenue} />
                    </Stats>
                    <Stats label="Storage Revenue" tooltip="Storage Revenue">
                        <SuiAmount amount={gasCostSummary?.storageRevenue} />
                    </Stats>
                    <Stats label="Stake Rewards" tooltip="Stake Rewards">
                        <SuiAmount amount={gasCostSummary?.stakeRewards} />
                    </Stats>
                </EpochStats>
                <EpochStats label="Rewards">
                    <Stats label="Stake Subsidies" tooltip="Stake Subsidies">
                        <SuiAmount amount={stakeSubsidies} />
                    </Stats>
                    <Stats label="Total Rewards" tooltip="Total Rewards">
                        <SuiAmount amount={totalRewards} />
                    </Stats>

                    <Stats
                        label="Storage Fund Earnings"
                        tooltip="Storage Fund Earnings"
                    >
                        <SuiAmount amount={storageFundEarnings} />
                    </Stats>
                </EpochStats>
                <Card spacing="lg">
                    <RingChart
                        title="Validators in Next Epoch"
                        suffix="validators"
                        data={[
                            {
                                value: active ?? 0,
                                label: 'Active',
                                color: '#589AEA',
                            },
                            {
                                value: pending ?? 0,
                                label: 'New',
                                color: '#6FBCF0',
                            },
                            {
                                value: atRisk ?? 0,
                                label: 'At Risk',
                                color: '#FF794B',
                            },
                        ]}
                    />
                </Card>
            </div>

            <TabGroup size="lg">
                <TabList>
                    <Tab>Checkpoints</Tab>
                    <Tab>Participating Validators</Tab>
                </TabList>
                <TabPanels className="mt-4">
                    <TabPanel>
                        <CheckpointsTable epoch={epochQuery.data.epoch} />
                    </TabPanel>
                    <TabPanel>
                        {validatorsTable ? (
                            <TableCard
                                data={validatorsTable?.data!}
                                sortTable
                                defaultSorting={[{ id: 'stake', desc: false }]}
                                columns={validatorsTable?.columns!}
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
    if (gb?.ready) {
        return <EpochDetail />;
    }
    return <LoadingSpinner />;
}
