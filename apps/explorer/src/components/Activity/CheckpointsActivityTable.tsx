// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromCheckpointsData } from './utils';

import { useGetCheckpoints } from '~/hooks/useGetCheckpoints';
import { Link } from '~/ui/Link';
import { Pagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { numberSuffix } from '~/utils/numberUtil';

const DEFAULT_CHECKPOINTS_LIMIT = 20;

interface Props {
    disablePagination?: boolean;
    refetchInterval?: number;
    initialLimit?: number;
}

export function CheckpointsActivityTable({
    disablePagination,
    initialLimit = DEFAULT_CHECKPOINTS_LIMIT,
}: Props) {
    const [currentPage, setCurrentPage] = useState(0);
    const [limit, setLimit] = useState(initialLimit);
    const rpc = useRpcClient();
    const {
        data,
        isLoading,
        isFetching,
        isFetchingNextPage,
        fetchNextPage,
        hasNextPage,
    } = useGetCheckpoints(limit);

    const countQuery = useQuery(['checkpoints', 'count'], () =>
        rpc.getLatestCheckpointSequenceNumber()
    );

    const cardData =
        data && Boolean(data.pages[currentPage])
            ? genTableDataFromCheckpointsData(data.pages[currentPage])
            : undefined;

    return (
        <div className="flex flex-col space-y-5 text-left xl:pr-10">
            {isLoading || isFetching || isFetchingNextPage || !cardData ? (
                <PlaceholderTable
                    rowCount={Number(limit)}
                    rowHeight="16px"
                    colHeadings={[
                        'Digest',
                        'Sequence Number',
                        'Time',
                        'Transaction Count',
                    ]}
                    colWidths={['100px', '120px', '204px', '90px', '38px']}
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
                {(hasNextPage || data?.pages.length) && !disablePagination ? (
                    <Pagination
                        onNext={() => {
                            if (isLoading || isFetching) {
                                return;
                            }

                            // Make sure we are at the end before fetching another page
                            if (
                                data &&
                                currentPage === data?.pages.length - 1 &&
                                !isLoading &&
                                !isFetching
                            ) {
                                fetchNextPage();
                            }
                            setCurrentPage(currentPage + 1);
                        }}
                        hasNext={
                            Boolean(hasNextPage) &&
                            Boolean(data?.pages[currentPage])
                        }
                        hasPrev={currentPage !== 0}
                        onPrev={() => setCurrentPage(currentPage - 1)}
                        onFirst={() => setCurrentPage(0)}
                    />
                ) : (
                    disablePagination && (
                        <Link
                            to="/recent?tab=checkpoints"
                            after={<ArrowRight12 />}
                        >
                            More Checkpoints
                        </Link>
                    )
                )}

                <div className="flex items-center space-x-3">
                    <Text variant="body/medium" color="steel-dark">
                        {countQuery.data
                            ? numberSuffix(Number(countQuery.data))
                            : '-'}
                        {` Checkpoints`}
                    </Text>
                    {!disablePagination && (
                        <select
                            className="form-select rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
                            value={limit}
                            onChange={(e) => {
                                setLimit(Number(e.target.value));
                                setCurrentPage(0);
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
    );
}
