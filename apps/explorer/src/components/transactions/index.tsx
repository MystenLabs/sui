// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient, useGetTotalTransactionBlocks } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { TableFooter } from '../Table/TableFooter';
import { genTableDataFromTxData } from './TxCardUtils';

import { Banner } from '~/ui/Banner';
import { usePaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';

type Props = {
    initialLimit: number;
    disablePagination?: boolean;
    refetchInterval?: number;
};

export function Transactions({
    initialLimit,
    disablePagination,
    refetchInterval,
}: Props) {
    const [limit, setLimit] = useState(initialLimit);

    const rpc = useRpcClient();

    const countQuery = useGetTotalTransactionBlocks();

    const pagination = usePaginationStack();

    const transactionQuery = useQuery(
        ['transactions', { limit, cursor: pagination.cursor }],
        async () =>
            rpc.queryTransactionBlocks({
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
            keepPreviousData: true,
            // Disable refetching if not on the first page:
            // refetchInterval: pagination.cursor ? undefined : refetchInterval,
            retry: false,
            staleTime: Infinity,
            cacheTime: 24 * 60 * 60 * 1000,
        }
    );

    const recentTx = useMemo(
        () =>
            transactionQuery.data
                ? genTableDataFromTxData(transactionQuery.data.data)
                : null,
        [transactionQuery.data]
    );

    if (transactionQuery.isError) {
        return (
            <Banner variant="error" fullWidth>
                There was an issue getting the latest transactions.
            </Banner>
        );
    }

    return (
        <div>
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
                    colHeadings={['Digest', 'Sender', 'Amount', 'Gas', 'Time']}
                    colWidths={['100px', '120px', '204px', '90px', '38px']}
                />
            )}

            <div className="py-3">
                <TableFooter
                    label="Transaction Blocks"
                    count={Number(countQuery.data)}
                    data={transactionQuery.data}
                    disablePagination={disablePagination}
                    pagination={pagination}
                    limit={limit}
                    onLimitChange={setLimit}
                    href="/recent"
                />
            </div>
        </div>
    );
}
