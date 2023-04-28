// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { useParams } from 'react-router-dom';

import { CheckpointTransactionBlocks } from './CheckpointTransactionBlocks';

import { SuiAmount } from '~/components/Table/SuiAmount';
import { Banner } from '~/ui/Banner';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { EpochLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';

export default function CheckpointDetail() {
    const { id } = useParams<{ id: string }>();
    const digestOrSequenceNumber = /^\d+$/.test(id!) ? parseInt(id!, 10) : id;

    const rpc = useRpcClient();
    const { data, isError, isLoading } = useQuery(['checkpoints', id], () =>
        rpc.getCheckpoint({ id: String(digestOrSequenceNumber!) })
    );

    if (isError)
        return (
            <Banner variant="error" fullWidth>
                There was an issue retrieving data for checkpoint: {id}
            </Banner>
        );
    if (isLoading) return <LoadingSpinner />;

    return (
        <div className="flex flex-col space-y-12">
            <PageHeader title={data.digest} type="Checkpoint" />
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
                                        variant="pBody/medium"
                                        color="steel-darker"
                                    >
                                        {data.sequenceNumber}
                                    </Text>
                                </DescriptionItem>
                                <DescriptionItem title="Epoch">
                                    <EpochLink epoch={data.epoch} />
                                </DescriptionItem>
                                <DescriptionItem title="Checkpoint Timestamp">
                                    <Text
                                        variant="pBody/medium"
                                        color="steel-darker"
                                    >
                                        {data.timestampMs
                                            ? new Date(
                                                  Number(data.timestampMs)
                                              ).toLocaleString(undefined, {
                                                  month: 'short',
                                                  day: 'numeric',
                                                  year: 'numeric',
                                                  hour: 'numeric',
                                                  minute: '2-digit',
                                                  second: '2-digit',
                                                  hour12: false,
                                                  timeZone: 'UTC',
                                                  timeZoneName: 'short',
                                              })
                                            : '--'}
                                    </Text>
                                </DescriptionItem>
                            </DescriptionList>
                        </TabPanel>
                        <TabPanel>
                            <TabGroup>
                                <TabList>
                                    <Tab>Aggregated Validator Signature</Tab>
                                </TabList>
                                <TabPanels>
                                    <DescriptionList>
                                        <DescriptionItem
                                            key={data.validatorSignature}
                                            title="Signature"
                                        >
                                            <Text
                                                variant="pBody/medium"
                                                color="steel-darker"
                                            >
                                                {data.validatorSignature}
                                            </Text>
                                        </DescriptionItem>
                                    </DescriptionList>
                                </TabPanels>
                            </TabGroup>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Gas & Storage Fees</Tab>
                    </TabList>
                    <TabPanels>
                        <DescriptionList>
                            <DescriptionItem title="Computation Fee">
                                <SuiAmount
                                    full
                                    amount={
                                        data.epochRollingGasCostSummary
                                            .computationCost
                                    }
                                />
                            </DescriptionItem>
                            <DescriptionItem title="Storage Fee">
                                <SuiAmount
                                    full
                                    amount={
                                        data.epochRollingGasCostSummary
                                            .storageCost
                                    }
                                />
                            </DescriptionItem>
                            <DescriptionItem title="Storage Rebate">
                                <SuiAmount
                                    full
                                    amount={
                                        data.epochRollingGasCostSummary
                                            .storageRebate
                                    }
                                />
                            </DescriptionItem>
                        </DescriptionList>
                    </TabPanels>
                </TabGroup>

                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Checkpoint Transaction Blocks</Tab>
                    </TabList>
                    <TabPanels>
                        <div className="mt-4">
                            <CheckpointTransactionBlocks
                                id={data.sequenceNumber}
                            />
                        </div>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}
