// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromTxData } from '../transactions/TxCardUtils';

import { useGetTransactionBlocks } from '~/hooks/useGetTransactionBlocks';
import { Link } from '~/ui/Link';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { numberSuffix } from '~/utils/numberUtil';

const DEFAULT_TRANSACTIONS_LIMIT = 20;

interface Props {
    disablePagination?: boolean;
    refetchInterval?: number;
    initialLimit?: number;
}

export function TransactionsActivityTable({
    disablePagination,
    initialLimit = DEFAULT_TRANSACTIONS_LIMIT,
}: Props) {
    const [limit, setLimit] = useState(initialLimit);
    const rpc = useRpcClient();
    const { data: count } = useQuery(
        ['transactions', 'count'],
        () => rpc.getTotalTransactionBlocks(),
        { cacheTime: 24 * 60 * 60 * 1000, staleTime: Infinity, retry: false }
    );

    const transactions = useGetTransactionBlocks(undefined, limit);
    const { data, isFetching, pagination, isLoading, isError } =
        useCursorPagination(transactions);

    const cardData = data ? genTableDataFromTxData(data.data) : undefined;

    return (
        <div data-testid="tx">
            {isError && (
                <div className="pt-2 font-sans font-semibold text-issue-dark">
                    Failed to load Transactions
                </div>
            )}
            <div className="flex flex-col space-y-5 text-left xl:pr-10">
                {isLoading || isFetching || !cardData ? (
                    <PlaceholderTable
                        rowCount={limit}
                        rowHeight="16px"
                        colHeadings={[
                            'Digest',
                            'Sender',
                            'Txns',
                            'Gas',
                            'Time',
                        ]}
                        colWidths={['30%', '30%', '10%', '20%', '10%']}
                    />
                ) : (
                    <div>
                        <TableCard
                            data={cardData.data}
                            columns={cardData.columns}
                        />
                    </div>
                )}

                <div className="flex justify-between">
                    {!disablePagination ? (
                        <Pagination {...pagination} />
                    ) : (
                        <Link to="/recent" after={<ArrowRight12 />}>
                            More Transaction Blocks
                        </Link>
                    )}

                    <div className="flex items-center space-x-3">
                        <Text variant="body/medium" color="steel-dark">
                            {count ? numberSuffix(Number(count)) : '-'}
                            {` Transaction Blocks`}
                        </Text>
                        {!disablePagination && (
                            <select
                                className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
                                value={limit}
                                onChange={(e) => {
                                    setLimit(Number(e.target.value));
                                    pagination.onFirst();
                                }}
                            >
                                <option value={20}>20 Per Page</option>
                                <option value={40}>40 Per Page</option>
                                <option value={60}>60 Per Page</option>
                            </select>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
