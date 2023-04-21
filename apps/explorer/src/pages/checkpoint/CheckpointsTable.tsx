// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';

import { TableFooter } from '~/components/Table/TableFooter';
import { TxTableCol } from '~/components/transactions/TxCardUtils';
import { TxTimeType } from '~/components/tx-time/TxTimeType';
import { CheckpointLink } from '~/ui/InternalLink';
import { useBoundedPaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

interface CheckpointsTableProps {
    initialLimit: number;
    initialCursor?: string;
    maxCursor?: string;
    disablePagination?: boolean;
    refetchInterval?: number;
}

export function CheckpointsTable({
    initialLimit,
    initialCursor,
    maxCursor,
    disablePagination,
}: CheckpointsTableProps) {
    const rpc = useRpcClient();
    const [limit, setLimit] = useState(initialLimit);
    const [cursor, setCursor] = useState(initialCursor);

    const countQuery = useQuery(['checkpoints', 'count'], () =>
        rpc.getLatestCheckpointSequenceNumber()
    );
    const pagination = useBoundedPaginationStack<string>(
        initialCursor,
        maxCursor
    );

    const count = useMemo(() => {
        if (maxCursor && initialCursor)
            return Number(initialCursor) - Number(maxCursor);
        return Number(countQuery.data ?? 0);
    }, [countQuery.data, initialCursor, maxCursor]);

    const { data: checkpointsData } = useQuery(
        ['checkpoints', { limit, cursor }],
        () =>
            rpc.getCheckpoints({
                limit:
                    cursor &&
                    maxCursor &&
                    Number(cursor) - limit < Number(maxCursor)
                        ? Number(cursor) - Number(maxCursor)
                        : limit,
                descendingOrder: true,
                cursor,
            }),
        {
            keepPreviousData: true,
            // Disable refetching if not on the first page:
            // refetchInterval: cursor ? undefined : refetchInterval,
            retry: false,
            staleTime: Infinity,
            cacheTime: 24 * 60 * 60 * 1000,
        }
    );

    useEffect(() => {
        if (pagination.cursor) {
            setCursor(pagination.cursor);
        }
    }, [pagination]);

    const checkpointsTable = useMemo(
        () =>
            checkpointsData
                ? {
                      data: checkpointsData?.data.map((checkpoint) => ({
                          digest: (
                              <TxTableCol isHighlightedOnHover>
                                  <CheckpointLink digest={checkpoint.digest} />
                              </TxTableCol>
                          ),
                          time: (
                              <TxTableCol>
                                  <TxTimeType
                                      timestamp={Number(checkpoint.timestampMs)}
                                  />
                              </TxTableCol>
                          ),
                          sequenceNumber: (
                              <TxTableCol>
                                  <Text
                                      variant="bodySmall/medium"
                                      color="steel-darker"
                                  >
                                      {checkpoint.sequenceNumber}
                                  </Text>
                              </TxTableCol>
                          ),
                          transactionBlockCount: (
                              <TxTableCol>
                                  <Text
                                      variant="bodySmall/medium"
                                      color="steel-darker"
                                  >
                                      {checkpoint.transactions.length}
                                  </Text>
                              </TxTableCol>
                          ),
                      })),
                      columns: [
                          {
                              header: 'Digest',
                              accessorKey: 'digest',
                          },
                          {
                              header: 'Sequence Number',
                              accessorKey: 'sequenceNumber',
                          },
                          {
                              header: 'Time',
                              accessorKey: 'time',
                          },
                          {
                              header: 'Transaction Block Count',
                              accessorKey: 'transactionBlockCount',
                          },
                      ],
                  }
                : null,
        [checkpointsData]
    );

    return (
        <div>
            {checkpointsTable ? (
                <TableCard
                    data={checkpointsTable.data}
                    columns={checkpointsTable.columns}
                />
            ) : (
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
            )}
            <div className="py-3">
                <TableFooter
                    label="Checkpoints"
                    data={checkpointsData}
                    count={count}
                    limit={Number(limit)}
                    onLimitChange={setLimit}
                    pagination={pagination}
                    disablePagination={disablePagination}
                    href="/recent?tab=checkpoints"
                />
            </div>
        </div>
    );
}
