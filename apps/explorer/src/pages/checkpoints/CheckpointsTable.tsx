// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { TableFooter } from '~/components/Table/TableFooter';
import { TxTableCol } from '~/components/transactions/TxCardUtils';
import { TxTimeType } from '~/components/tx-time/TxTimeType';
import { CheckpointLink } from '~/ui/InternalLink';
import { usePaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

interface CheckpointsTableProps {
    initialLimit: number;
    disablePagination?: boolean;
    refetchInterval?: number;
}

export function CheckpointsTable({
    initialLimit,
    disablePagination,
    refetchInterval,
}: CheckpointsTableProps) {
    const rpc = useRpcClient();
    const [limit, setLimit] = useState(initialLimit);

    const countQuery = useQuery(['checkpoints', 'count'], () =>
        rpc.getLatestCheckpointSequenceNumber()
    );

    const pagination = usePaginationStack<string>();

    const { data: checkpointsData } = useQuery(
        ['checkpoints', { limit, cursor: pagination.cursor }],
        () =>
            rpc.getCheckpoints({
                limit,
                descendingOrder: true,
                cursor: pagination.cursor,
            }),
        {
            keepPreviousData: true,
            // Disable refetching if not on the first page:
            refetchInterval: pagination.cursor ? undefined : refetchInterval,
        }
    );

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
                                      timestamp={checkpoint.timestampMs}
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
                    rowCount={limit}
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
                    count={Number(countQuery.data)}
                    limit={limit}
                    onLimitChange={setLimit}
                    pagination={pagination}
                    disablePagination={disablePagination}
                    href="/recent?tab=checkpoints"
                />
            </div>
        </div>
    );
}
