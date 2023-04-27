// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { genTableDataFromCheckpointsData } from './utils';

import { useGetCheckpoints } from '~/hooks/useGetCheckpoints';
import { Link } from '~/ui/Link';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { numberSuffix } from '~/utils/numberUtil';

const DEFAULT_CHECKPOINTS_LIMIT = 20;

interface Props {
    disablePagination?: boolean;
    refetchInterval?: number;
    initialLimit?: number;
    initialCursor?: string;
    maxCursor?: string;
}

export function CheckpointsTable({
    disablePagination,
    initialLimit = DEFAULT_CHECKPOINTS_LIMIT,
    initialCursor,
    maxCursor,
}: Props) {
    const [limit, setLimit] = useState(initialLimit);
    const rpc = useRpcClient();

    const countQuery = useQuery(['checkpoints', 'count'], () =>
        rpc.getLatestCheckpointSequenceNumber()
    );

    const count = useMemo(() => {
        if (maxCursor && initialCursor)
            return Number(initialCursor) - Number(maxCursor);
        return Number(countQuery.data ?? 0);
    }, [countQuery.data, initialCursor, maxCursor]);

    const checkpoints = useGetCheckpoints(initialCursor, limit);

    const { data, isFetching, pagination, isLoading, isError } =
        useCursorPagination(checkpoints);
    const cardData = data ? genTableDataFromCheckpointsData(data) : undefined;

    return (
        <div className="flex flex-col space-y-5 text-left xl:pr-10">
            {isError && (
                <div className="pt-2 font-sans font-semibold text-issue-dark">
                    Failed to load Checkpoints
                </div>
            )}
            {isLoading || isFetching || !cardData ? (
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
                {!disablePagination ? (
                    <Pagination
                        {...pagination}
                        hasNext={
                            maxCursor
                                ? Number(data && data.nextCursor) >
                                  Number(maxCursor)
                                : pagination.hasNext
                        }
                    />
                ) : (
                    <Link to="/recent?tab=checkpoints" after={<ArrowRight12 />}>
                        More Checkpoints
                    </Link>
                )}

                <div className="flex items-center space-x-3">
                    <Text variant="body/medium" color="steel-dark">
                        {count ? numberSuffix(Number(count)) : '-'}
                        {` Checkpoints`}
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
    );
}
