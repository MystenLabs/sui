// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

// import { EpochTimer } from './EpochTimer';
import { getEpochs } from './mocks';

import { SuiAmount } from '~/components/transaction-card/TxCardUtils';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

export function EpochsTable() {
    // todo: remove mock data and replace with rpc call
    const { data: epochs } = useQuery(['epochs'], () => getEpochs());

    const tableData = useMemo(
        () =>
            epochs
                ? {
                      data: epochs?.map((epoch: any) => ({
                          epoch: (
                              <Text variant="bodySmall/medium">
                                  {epoch.epoch}
                              </Text>
                          ),
                          transactions: (
                              <Text variant="bodySmall/medium">
                                  {epoch.transactionCount}
                              </Text>
                          ),
                          stakeRewards: (
                              <SuiAmount
                                  amount={epoch.gasCostSummary.totalRevenue}
                              />
                          ),
                          checkpointSet: (
                              <Text variant="bodySmall/medium">
                                  {epoch.checkpointSet?.join(' - ')}
                              </Text>
                          ),
                          storageRevenue: (
                              <SuiAmount
                                  amount={epoch.gasCostSummary.storageRevenue}
                              />
                          ),
                      })),
                      columns: [
                          { header: 'Epoch', accessorKey: 'epoch' },
                          {
                              header: 'Transactions',
                              accessorKey: 'transactions',
                          },
                          {
                              header: 'Checkpoint Set',
                              accessorKey: 'checkpointSet',
                          },
                          {
                              header: 'Stake Rewards',
                              accessorKey: 'stakeRewards',
                          },
                          {
                              header: 'Storage Revenue',
                              accessorKey: 'storageRevenue',
                          },
                      ],
                  }
                : null,
        [epochs]
    );

    return (
        <div className="flex flex-col items-center justify-center gap-6">
            {/* todo: add pagination and enable timer  */}
            {/* <EpochTimer /> */}
            {tableData ? (
                <TableCard data={tableData.data} columns={tableData.columns} />
            ) : (
                <PlaceholderTable
                    rowCount={20}
                    rowHeight="13px"
                    colHeadings={['time', 'number']}
                    colWidths={['50%', '50%']}
                />
            )}
        </div>
    );
}
