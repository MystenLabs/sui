// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import toast from 'react-hot-toast';

import { genTableDataFromTxData } from './TxCardUtils';

import { CheckpointsTable } from '~/pages/checkpoints/CheckpointsTable';
import { Banner } from '~/ui/Banner';
import { Link } from '~/ui/Link';
import { Pagination, usePaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { PlayPause } from '~/ui/PlayPause';
import { TableCard } from '~/ui/TableCard';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
import { numberSuffix } from '~/utils/numberUtil';

const TRANSACTION_POLL_TIME_SECONDS = 10;
const TRANSACTION_POLL_TIME = TRANSACTION_POLL_TIME_SECONDS * 1000;

const AUTO_REFRESH_ID = 'auto-refresh';

type Props = {
    initialLimit: number;
    disablePagination?: boolean;
};

export function LatestTxCard({ initialLimit, disablePagination }: Props) {
    const [paused, setPaused] = useState(false);
    const [limit, setLimit] = useState(initialLimit);

    const rpc = useRpcClient();

    const countQuery = useQuery(['transactions', 'count'], () =>
        rpc.getTotalTransactionNumber()
    );

    const pagination = usePaginationStack();
    const refetching = !pagination.cursor && !paused;

    const transactionQuery = useQuery(
        ['transactions', { limit, cursor: pagination.cursor }],
        async () =>
            rpc.queryTransactions({
                order: 'descending',
                cursor: pagination.cursor,
                limit,
                options: {
                    showEffects: true,
                    showBalanceChanges: true,
                    showInput: true,
                },
            }),
        {
            enabled: countQuery.isFetched,
            keepPreviousData: true,
            refetchInterval: refetching ? TRANSACTION_POLL_TIME : false,
        }
    );

    const recentTx = useMemo(
        () =>
            transactionQuery.data
                ? genTableDataFromTxData(transactionQuery.data.data)
                : null,
        [transactionQuery.data]
    );

    const handlePauseChange = () => {
        if (paused) {
            // If we were paused, and on the first page, immediately refetch:
            if (!pagination.cursor) {
                countQuery.refetch();
            }
            toast.success(
                `Auto-refreshing on - every ${TRANSACTION_POLL_TIME_SECONDS} seconds`,
                { id: AUTO_REFRESH_ID }
            );
        } else {
            toast.success('Auto-refresh paused', { id: AUTO_REFRESH_ID });
        }

        setPaused((paused) => !paused);
    };

    if (transactionQuery.isError) {
        return (
            <Banner variant="error" fullWidth>
                There was an issue getting the latest transactions.
            </Banner>
        );
    }

    return (
        <div>
            <TabGroup size="lg">
                <div className="relative flex items-center">
                    <TabList>
                        <Tab>Transactions</Tab>
                        <Tab>Checkpoints</Tab>
                    </TabList>

                    <div className="absolute inset-y-0 right-0 text-2xl">
                        <PlayPause
                            paused={paused}
                            onChange={handlePauseChange}
                        />
                    </div>
                </div>
                <TabPanels>
                    <TabPanel>
                        {recentTx ? (
                            <TableCard
                                refetching={transactionQuery.isPreviousData}
                                data={recentTx.data}
                                columns={recentTx.columns}
                            />
                        ) : (
                            <PlaceholderTable
                                rowCount={initialLimit}
                                rowHeight="16px"
                                colHeadings={[
                                    'Transaction ID',
                                    'Sender',
                                    'Amount',
                                    'Gas',
                                    'Time',
                                ]}
                                colWidths={[
                                    '100px',
                                    '120px',
                                    '204px',
                                    '90px',
                                    '38px',
                                ]}
                            />
                        )}

                        <div className="flex items-center justify-between py-3">
                            {disablePagination ? (
                                <>
                                    <Link
                                        to="/transactions"
                                        after={<ArrowRight12 />}
                                    >
                                        More Transactions
                                    </Link>
                                    <Text
                                        variant="body/medium"
                                        color="steel-dark"
                                    >
                                        {countQuery.data
                                            ? numberSuffix(countQuery.data)
                                            : '-'}{' '}
                                        Transactions
                                    </Text>
                                </>
                            ) : (
                                <>
                                    <Pagination
                                        {...pagination.props(
                                            transactionQuery.data
                                        )}
                                    />
                                    <div className="flex items-center gap-4">
                                        <Text
                                            variant="body/medium"
                                            color="steel-dark"
                                        >
                                            {countQuery.data
                                                ? numberSuffix(countQuery.data)
                                                : '-'}{' '}
                                            Transactions
                                        </Text>

                                        <select
                                            className="shadow-button form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark"
                                            value={limit}
                                            onChange={(e) =>
                                                setLimit(+e.target.value)
                                            }
                                        >
                                            <option value={20}>
                                                20 Per Page
                                            </option>
                                            <option value={40}>
                                                40 Per Page
                                            </option>
                                            <option value={60}>
                                                60 Per Page
                                            </option>
                                        </select>
                                    </div>
                                </>
                            )}
                        </div>
                    </TabPanel>

                    <TabPanel>
                        <CheckpointsTable
                            initialItemsPerPage={initialLimit}
                            refetchInterval={TRANSACTION_POLL_TIME}
                            shouldRefetch={!paused}
                            // paginationType={paginationtype}
                        />
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}
