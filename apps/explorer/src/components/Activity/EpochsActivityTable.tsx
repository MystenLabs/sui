// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromEpochsData } from './utils';

import { useGetEpochs } from '~/hooks/useGetEpochs';
import { Link } from '~/ui/Link';
import { Pagination } from '~/ui/Pagination';
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

export function EpochsActivityTable({
    disablePagination,
    initialLimit = DEFAULT_TRANSACTIONS_LIMIT,
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
    } = useGetEpochs(limit);

    const countQuery = useQuery(
        ['epochs', 'current'],
        async () => rpc.getCurrentEpoch(),
        {
            select: (epoch) => Number(epoch.epoch) + 1,
        }
    );

    const cardData =
        data && Boolean(data.pages[currentPage])
            ? genTableDataFromEpochsData(data.pages[currentPage])
            : undefined;

    return (
        <div data-testid="tx">
            <div className="flex flex-col space-y-5 text-left xl:pr-10">
                {isLoading || isFetching || isFetchingNextPage || !cardData ? (
                    <PlaceholderTable
                        rowCount={limit}
                        rowHeight="16px"
                        colHeadings={[
                            'Epoch',
                            'Transaction Blocks',
                            'Stake Rewards',
                            'Checkpoint Set',
                            'Storage Revenue',
                            'Epoch End',
                        ]}
                        colWidths={[
                            '100px',
                            '120px',
                            '40px',
                            '204px',
                            '90px',
                            '38px',
                        ]}
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
                    {(hasNextPage || data?.pages.length) &&
                    !disablePagination ? (
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
                            hasNext={Boolean(hasNextPage)}
                            hasPrev={currentPage !== 0}
                            onPrev={() => setCurrentPage(currentPage - 1)}
                            onFirst={() => setCurrentPage(0)}
                        />
                    ) : (
                        <div>
                            {disablePagination && (
                                <Link
                                    to="/recent?tab=epochs"
                                    after={<ArrowRight12 />}
                                >
                                    More Epochs
                                </Link>
                            )}
                        </div>
                    )}

                    <div className="flex items-center space-x-3">
                        <Text variant="body/medium" color="steel-dark">
                            {countQuery.data
                                ? numberSuffix(Number(countQuery.data))
                                : '-'}
                            {` Epochs`}
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
        </div>
    );
}
