// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature, useGrowthBook } from '@growthbook/growthbook-react';
import { useQuery } from '@tanstack/react-query';
import { Navigate, useParams } from 'react-router-dom';

import {
    genTableDataFromTxData,
    getDataOnTxDigests,
    type TxnData,
} from '~/components/transaction-card/TxCardUtils';
import { useRpc } from '~/hooks/useRpc';
import { Banner } from '~/ui/Banner';
import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';
import { convertNumberToDate } from '~/utils/timeUtils';

function CheckpointDetail() {
    const enabled = useFeature(GROWTHBOOK_FEATURES.EPOCHS_CHECKPOINTS).on;
    const { digest } = useParams<{ digest: string }>();
    const rpc = useRpc();

    const checkpointQuery = useQuery(
        ['checkpoints', digest],
        async () => await rpc.getCheckpoint(digest!)
    );

    // todo: add user_signatures to combined `getCheckpoint` endpoint
    const contentsQuery = useQuery(
        ['contents'],
        async () => await rpc.getCheckpointContents(checkpoint.sequenceNumber),
        { enabled: !!checkpointQuery.data }
    );

    const { data: transactions } = useQuery(
        ['checkpoint-transactions'],
        async () => {
            // todo: replace this with `sui_getTransactions` call when we are
            // able to query by checkpoint digest
            const txData = await getDataOnTxDigests(
                rpc,
                checkpointQuery.data?.transactions!
            );
            return genTableDataFromTxData(txData as TxnData[]);
        },
        { enabled: checkpointQuery.isFetched }
    );

    if (!enabled) return <Navigate to="/" />;

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
                                    (signature) => (
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
                            {transactions?.data ? (
                                <TableCard
                                    data={transactions?.data}
                                    columns={transactions?.columns}
                                />
                            ) : null}
                        </div>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}

export default () => {
    const gb = useGrowthBook();
    if (gb?.ready) {
        return <CheckpointDetail />;
    }
    return <LoadingSpinner />;
};
