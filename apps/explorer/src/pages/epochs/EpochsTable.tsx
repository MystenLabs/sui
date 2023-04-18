// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { SuiAmount } from '~/components/Table/SuiAmount';
import { TableFooter } from '~/components/Table/TableFooter';
import { TxTableCol } from '~/components/transactions/TxCardUtils';
import { TxTimeType } from '~/components/tx-time/TxTimeType';
import { useEnhancedRpcClient } from '~/hooks/useEnhancedRpc';
import { CheckpointSequenceLink, EpochLink } from '~/ui/InternalLink';
import { usePaginationStack } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

interface EpochsTableProps {
    initialLimit: number;
    disablePagination?: boolean;
    refetchInterval?: number;
}

export function EpochsTable({
    initialLimit,
    disablePagination,
}: EpochsTableProps) {
    const enhancedRpc = useEnhancedRpcClient();
    const [limit, setLimit] = useState(initialLimit);

    const countQuery = useQuery(
        ['epochs', 'current'],
        async () => enhancedRpc.getCurrentEpoch(),
        {
            select: (epoch) => Number(epoch.epoch) + 1,
        }
    );

    const pagination = usePaginationStack<number>();

    const { data: epochsData } = useQuery(
        ['epochs', { limit, cursor: pagination.cursor }],
        async () =>
            enhancedRpc.getEpochs({
                limit,
                cursor: pagination.cursor?.toString(),
                descendingOrder: true,
            }),
        {
            keepPreviousData: true,
            retry: 5,
            // Disable refetching if not on the first page:
            // refetchInterval: pagination.cursor ? undefined : refetchInterval,
            staleTime: Infinity,
            cacheTime: 24 * 60 * 60 * 1000,
        }
    );

    const epochsTable = useMemo(
        () =>
            epochsData
                ? {
                      data: epochsData?.data.map((epoch) => ({
                          epoch: (
                              <TxTableCol isHighlightedOnHover>
                                  <EpochLink epoch={epoch.epoch.toString()} />
                              </TxTableCol>
                          ),
                          transactions: (
                              <TxTableCol>
                                  <Text variant="bodySmall/medium">
                                      {epoch.epochTotalTransactions}
                                  </Text>
                              </TxTableCol>
                          ),
                          stakeRewards: (
                              <TxTableCol>
                                  <SuiAmount
                                      amount={
                                          epoch.endOfEpochInfo
                                              ?.totalStakeRewardsDistributed
                                      }
                                  />
                              </TxTableCol>
                          ),
                          checkpointSet: (
                              <div>
                                  <CheckpointSequenceLink
                                      sequence={epoch.firstCheckpointId.toString()}
                                  />
                                  {` - `}
                                  <CheckpointSequenceLink
                                      sequence={
                                          epoch.endOfEpochInfo?.lastCheckpointId.toString() ??
                                          ''
                                      }
                                  />
                              </div>
                          ),
                          storageRevenue: (
                              <TxTableCol>
                                  <SuiAmount
                                      amount={
                                          epoch.endOfEpochInfo?.storageCharge
                                      }
                                  />
                              </TxTableCol>
                          ),
                          time: (
                              <TxTableCol>
                                  <TxTimeType
                                      timestamp={Number(
                                          epoch.endOfEpochInfo
                                              ?.epochEndTimestamp ?? 0
                                      )}
                                  />
                              </TxTableCol>
                          ),
                      })),
                      columns: [
                          {
                              header: 'Epoch',
                              accessorKey: 'epoch',
                          },
                          {
                              header: 'Transaction Blocks',
                              accessorKey: 'transactions',
                          },
                          {
                              header: 'Stake Rewards',
                              accessorKey: 'stakeRewards',
                          },
                          {
                              header: 'Checkpoint Set',
                              accessorKey: 'checkpointSet',
                          },
                          {
                              header: 'Storage Revenue',
                              accessorKey: 'storageRevenue',
                          },
                          {
                              header: 'Epoch End',
                              accessorKey: 'time',
                          },
                      ],
                  }
                : null,
        [epochsData]
    );

    return (
        <div>
            {/* TODO: fix timer between epoch boundaries
                    <div className="pb-4">
                    <EpochTimer />
                </div> */}
            {epochsTable ? (
                <TableCard
                    data={epochsTable.data}
                    columns={epochsTable.columns}
                />
            ) : (
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
            )}
            <div className="py-3">
                <TableFooter
                    href="/recent?tab=epochs"
                    label="Epochs"
                    data={epochsData}
                    count={Number(countQuery.data ?? 0)}
                    limit={limit}
                    onLimitChange={setLimit}
                    pagination={pagination}
                    disablePagination={disablePagination}
                />
            </div>
        </div>
    );
}
