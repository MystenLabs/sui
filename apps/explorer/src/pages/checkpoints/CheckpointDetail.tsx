// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useFeature,
    FeaturesReady,
    useGrowthBook,
} from '@growthbook/growthbook-react';
import { type TransactionKindName } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { Navigate, useParams } from 'react-router-dom';

import { genTableDataFromTxData } from '~/components/transaction-card/TxCardUtils';
import { useRpc } from '~/hooks/useRpc';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanels } from '~/ui/Tabs';
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

    // const txQuery = useQuery(
    //     ['checkpoint-transactions'],
    //     async () =>
    //         // todo: replace this with `sui_getTransactions` call when we are
    //         // able to query by checkpoint digest
    //         await rpc.getTransactionWithEffectsBatch(checkpoint.transactions),
    //     { enabled: !!checkpoint?.transactions?.length }
    // );

    // // todo: loading/error states
    // if (checkpointsLoading || txQuery.isLoading) {
    //     return <div>loading...</div>;
    // }

    // if (txQuery.isError) {
    //     return <div>error</div>;
    // }

    // const { sequenceNumber, epoch, timestampMs, epochRollingGasCostSummary } =
    //     checkpoint;

    // // todo: this is placeholder data
    // const txDataForTable = txQuery.data?.map((tx) => ({
    //     From: tx.certificate.data.sender,
    //     To: Object.values(txQuery.data[0].certificate.data.transactions[0])[0]
    //         .recipients[0],
    //     txId: tx.certificate.transactionDigest,
    //     status: 'success' as 'success' | 'failure',
    //     txGas:
    //         tx.effects.gasUsed.computationCost +
    //         tx.effects.gasUsed.storageCost -
    //         tx.effects.gasUsed.storageRebate,
    //     suiAmount: 0,
    //     coinType: 'sui',
    //     kind: Object.keys(
    //         txQuery.data[0].certificate.data.transactions[0]
    //     )[0] as TransactionKindName,
    //     timestamp_ms: tx.timestamp_ms ?? Date.now(),
    // }));

    // const txTableData = genTableDataFromTxData(txDataForTable!, 10);

    if (!enabled) return <Navigate to="/" />;

    if (checkpointQuery.isError)
        return (
            <Banner variant="error" fullWidth>
                There was an issue retrieving checkpoint {id}
            </Banner>
        );

    if (checkpointQuery.isLoading) return <LoadingSpinner />;

    const {
        data: { epochRollingGasCostSummary, ...checkpoint },
    } = checkpointQuery;

    return (
        <div className="flex flex-col space-y-12">
            <PageHeader title={checkpoint.digest} type="Checkpoint" />
            <div className="space-y-10">
                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Details</Tab>
                    </TabList>
                    <TabPanels>
                        <dl className="mt-4 space-y-2">
                            <div className="space-y-2 sm:grid sm:grid-cols-4 sm:gap-4 sm:space-y-0">
                                <dt>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        Checkpoint Sequence No.
                                    </Text>
                                </dt>
                                <dd>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        {checkpoint.sequenceNumber}
                                    </Text>
                                </dd>
                            </div>
                            <div className="sm:grid sm:grid-cols-5 sm:gap-4">
                                <dt>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        Epoch
                                    </Text>
                                </dt>
                                <dd>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        {checkpoint.epoch}
                                    </Text>
                                </dd>
                            </div>
                            <div className="sm:grid sm:grid-cols-5 sm:gap-4">
                                <dt>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        Checkpoint Timestamp
                                    </Text>
                                </dt>
                                <dd>
                                    <Text
                                        color="steel-darker"
                                        variant="p1/medium"
                                    >
                                        {convertNumberToDate(
                                            checkpoint.timestampMs
                                        )}
                                    </Text>
                                </dd>
                            </div>
                        </dl>
                    </TabPanels>
                </TabGroup>
                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Gas & Storage Fee</Tab>
                    </TabList>
                    <TabPanels>
                        <div className="mt-4 max-w-md space-y-2 overflow-auto">
                            <div className="grid grid-cols-2">
                                <Text color="steel-darker" variant="p1/medium">
                                    Computation Fee
                                </Text>
                                <Text color="steel-darker" variant="p1/medium">
                                    {
                                        epochRollingGasCostSummary.computation_cost
                                    }
                                </Text>
                            </div>
                            <div className="grid grid-cols-2">
                                <Text color="steel-darker" variant="p1/medium">
                                    Storage Fee
                                </Text>
                                <Text color="steel-darker" variant="p1/medium">
                                    {epochRollingGasCostSummary.storage_cost}
                                </Text>
                            </div>
                            <div className="grid grid-cols-2">
                                <Text color="steel-darker" variant="p1/medium">
                                    Storage Rebate
                                </Text>
                                <Text color="steel-darker" variant="p1/medium">
                                    {epochRollingGasCostSummary.storage_rebate}
                                </Text>
                            </div>
                        </div>
                    </TabPanels>
                </TabGroup>

                <TabGroup as="div" size="lg">
                    <TabList>
                        <Tab>Checkpoint Transactions</Tab>
                    </TabList>
                    <TabPanels>
                        <div className="mt-4">
                            {/* <TableCard
                                data={txTableData.data}
                                columns={txTableData.columns}
                            /> */}
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
    } else {
        return <LoadingSpinner />;
    }
};
