// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { Text } from '~/ui/Text';
import { TableFooter } from '../Table/TableFooter';
import { genTableDataFromTxData } from './TxCardUtils';

import { Banner } from '~/ui/Banner';
import { Pagination, usePaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { useGetTransactionBlocks } from '~/hooks/useGetTransactionBlocks';
import { numberSuffix } from '~/utils/numberUtil';
import { ArrowRight12 } from '@mysten/icons';
import { Link } from '~/ui/Link';

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
    const [currentPage, setCurrentPage] = useState(0)
    const [limit, setLimit] = useState(initialLimit);

    const rpc = useRpcClient();

    const countQuery = useQuery(
        ['transactions', 'count'],
        () => rpc.getTotalTransactionBlocks(),
        { cacheTime: 24 * 60 * 60 * 1000, staleTime: Infinity, retry: false }
    );

    const {
        data,
        isError,
        isPreviousData,
        isLoading,
        isFetching,
        isFetchingNextPage,
        fetchNextPage,
        hasNextPage,
    } = useGetTransactionBlocks(undefined, limit);

    const recentTx = useMemo(
        () =>
            data
                ? genTableDataFromTxData(data.pages[currentPage].data)
                : null,
        [data]
    );

    if (isError) {
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
                    refetching={isPreviousData}
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

            {disablePagination ? <div className="flex items-center justify-between mt-3">
                <Link to={'/recent'} after={<ArrowRight12 />}>
                    More {'transactions'}
                </Link>
                <Text variant="body/medium" color="steel-dark">
                    {countQuery.data ? numberSuffix(Number(countQuery.data)) : '-'} {'transactions'}
                </Text>
            </div> : <div className="py-3 flex justify-between">
                {(hasNextPage || (data && data?.pages.length > 1)) && (
                    <Pagination
                        onNext={() => {
                            if (isLoading || isFetching) {
                                return;
                            }

                            // Make sure we are at the end before fetching another page
                            if (
                                data &&
                                currentPage ===
                                data?.pages.length - 1 &&
                                !isLoading &&
                                !isFetching
                            ) {
                                fetchNextPage();
                            }
                            fetchNextPage()
                            setCurrentPage(currentPage + 1);
                        }}
                        hasNext={Boolean(hasNextPage)}
                        hasPrev={currentPage !== 0}
                        onPrev={() =>
                            setCurrentPage(currentPage - 1)

                        }
                        onFirst={() =>
                            setCurrentPage(0)
                        }
                    />
                )}
                <div className="flex items-center space-x-2">
                    <Text variant="body/medium" color="steel-dark">
                        {countQuery.data ? numberSuffix(Number(countQuery.data)) : '-'} {'Transactions'}
                    </Text>

                    <select
                        className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
                        value={limit}
                        onChange={(e) =>
                            setLimit(Number(e.target.value))
                        }
                    >
                        <option value={20}>20 Per Page</option>
                        <option value={40}>40 Per Page</option>
                        <option value={60}>60 Per Page</option>
                    </select>
                </div>
            </div>}
        </div>
    );
}
