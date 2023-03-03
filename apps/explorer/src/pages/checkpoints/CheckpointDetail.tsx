// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature, useGrowthBook } from '@growthbook/growthbook-react';
import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { Navigate, useParams } from 'react-router-dom';

import { CheckpointTransactions } from './Transactions';

import { Banner } from '~/ui/Banner';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';
import { convertNumberToDate } from '~/utils/timeUtils';

function CheckpointDetail() {
    const { digest } = useParams<{ digest: string }>();
    const rpc = useRpcClient();

    const checkpointQuery = useQuery(['checkpoints', digest], () =>
        rpc.getCheckpoint(digest!)
    );

    // todo: add user_signatures to combined `getCheckpoint` endpoint
    const contentsQuery = useQuery(
        ['checkpoints', digest, 'contents'],
        () => rpc.getCheckpointContents(checkpoint.sequenceNumber),
        { enabled: !!checkpointQuery.data }
    );

    if (checkpointQuery.isError)
        return (
            <Banner variant="error" fullWidth>
                There was an issue retrieving data for checkpoint: {digest}
            </Banner>
        );

    if (checkpointQuery.isLoading) return <LoadingSpinner />;

    const {
        data: { epochRollingGasCostSummary, ...checkpoint },
    } = checkpointQuery;

    return (
        <div className="flex flex-col space-y-12">
            <PageHeader title={checkpoint.digest} type="Checkpoint" />
            <div className="space-y-8">
                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Details</Tab>
                        <Tab>Signatures</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <DescriptionList>
                                <DescriptionItem title="Checkpoint Sequence No.">
                                    <Text
                                        variant="p1/medium"
                                        color="steel-darker"
                                    >
                                        {checkpoint.sequenceNumber}
                                    </Text>
                                </DescriptionItem>
                                <DescriptionItem title="Epoch">
                                    <Text
                                        variant="p1/medium"
                                        color="steel-darker"
                                    >
                                        {checkpoint.epoch}
                                    </Text>
                                </DescriptionItem>
                                <DescriptionItem title="Checkpoint Timestamp">
                                    <Text
                                        variant="p1/medium"
                                        color="steel-darker"
                                    >
                                        {checkpoint.timestampMs
                                            ? convertNumberToDate(
                                                  checkpoint.timestampMs
                                              )
                                            : '--'}
                                    </Text>
                                </DescriptionItem>
                            </DescriptionList>
                        </TabPanel>
                        <TabPanel>
                            <DescriptionList>
                                {contentsQuery.data?.user_signatures.map(
                                    ([signature]) => (
                                        <DescriptionItem
                                            key={signature}
                                            title="Signature"
                                        >
                                            <Text
                                                variant="p1/medium"
                                                color="steel-darker"
                                            >
                                                {signature}
                                            </Text>
                                        </DescriptionItem>
                                    )
                                )}
                            </DescriptionList>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Gas & Storage Fee</Tab>
                    </TabList>
                    <TabPanels>
                        <DescriptionList>
                            <DescriptionItem title="Computation Fee">
                                <Text variant="p1/medium" color="steel-darker">
                                    {
                                        epochRollingGasCostSummary.computation_cost
                                    }
                                </Text>
                            </DescriptionItem>
                            <DescriptionItem title="Storage Fee">
                                <Text variant="p1/medium" color="steel-darker">
                                    {epochRollingGasCostSummary.storage_cost}
                                </Text>
                            </DescriptionItem>
                            <DescriptionItem title="Storage Rebate">
                                <Text variant="p1/medium" color="steel-darker">
                                    {epochRollingGasCostSummary.storage_rebate}
                                </Text>
                            </DescriptionItem>
                        </DescriptionList>
                    </TabPanels>
                </TabGroup>

                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Checkpoint Transactions</Tab>
                    </TabList>
                    <TabPanels>
                        <div className="mt-4">
                            <CheckpointTransactions
                                digest={checkpoint.digest}
                                transactions={checkpoint.transactions || []}
                            />
                        </div>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}

export default function CheckpointDetailFeatureFlagged() {
    const gb = useGrowthBook();
    const enabled = useFeature(GROWTHBOOK_FEATURES.EPOCHS_CHECKPOINTS).on;
    if (gb?.ready) {
        return enabled ? <CheckpointDetail /> : <Navigate to="/" />;
    }
    return <LoadingSpinner />;
}
