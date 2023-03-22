// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRpcClient } from '@mysten/core';
import { ArrowRight12 } from '@mysten/icons';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import Pagination from '~/components/pagination/Pagination';
import TabFooter from '~/components/tabs/TabFooter';
import {
    statusToLoadState,
    type PaginationType,
} from '~/components/transaction-card/RecentTxCard';
import { TxTableCol } from '~/components/transaction-card/TxCardUtils';
import { TxTimeType } from '~/components/tx-time/TxTimeType';
import { CheckpointLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

interface CheckpointsTableProps {
    initialItemsPerPage?: number;
    paginationType?: PaginationType;
    shouldRefetch?: boolean;
    refetchInterval: number;
}

export function CheckpointsTable({
    initialItemsPerPage,
    paginationType,
    shouldRefetch = false,
    refetchInterval,
}: CheckpointsTableProps) {
    const rpc = useRpcClient();
    const [itemsPerPage, setItemsPerPage] = useState(initialItemsPerPage || 20);
    const [currentPage, setCurrentPage] = useState(1);

    const countQuery = useQuery(
        ['checkpoints', 'count'],
        () => rpc.getLatestCheckpointSequenceNumber(),
        { refetchInterval: shouldRefetch ? refetchInterval : false }
    );

    const { data: checkpointsData } = useQuery(
        ['checkpoints', { total: countQuery.data, itemsPerPage, currentPage }],
        () =>
            rpc.getCheckpoints({
                limit: itemsPerPage,
                descendingOrder: true,
                cursor: countQuery.data! - currentPage - 1 * itemsPerPage,
            }),
        { enabled: countQuery.isFetched, keepPreviousData: true }
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
                          transactionCount: (
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
                              header: 'Transaction Count',
                              accessorKey: 'transactionCount',
                          },
                      ],
                  }
                : null,
        [checkpointsData]
    );

    const stats = {
        count: countQuery.data || 0,
        stats_text: 'Total Checkpoints',
        loadState: statusToLoadState[countQuery.status],
    };

    return checkpointsTable ? (
        <>
            <TableCard
                data={checkpointsTable.data}
                columns={checkpointsTable.columns}
            />
            {paginationType === 'pagination' ? (
                <Pagination
                    totalItems={countQuery.data || 0}
                    itemsPerPage={itemsPerPage}
                    updateItemsPerPage={setItemsPerPage}
                    onPagiChangeFn={(newPage) => setCurrentPage(newPage)}
                    currentPage={currentPage}
                    stats={stats}
                />
            ) : (
                <div className="mt-3">
                    <TabFooter stats={stats}>
                        <div className="w-full">
                            <Link to="/transactions">
                                <div className="flex items-center gap-2">
                                    More Checkpoints{' '}
                                    <ArrowRight12 fill="currentColor" />
                                </div>
                            </Link>
                        </div>
                    </TabFooter>
                </div>
            )}
        </>
    ) : (
        <PlaceholderTable
            rowCount={itemsPerPage}
            rowHeight="16px"
            colHeadings={[
                'Digest',
                'Sequence Number',
                'Time',
                'Transaction Count',
            ]}
            colWidths={['100px', '120px', '204px', '90px', '38px']}
        />
    );
}
