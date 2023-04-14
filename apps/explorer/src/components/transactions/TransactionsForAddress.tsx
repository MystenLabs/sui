// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiTransactionBlockResponse } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { genTableDataFromTxData } from './TxCardUtils';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { TableCard } from '~/ui/TableCard';
import { TabGroup, TabList, Tab, TabPanels, TabPanel } from '~/ui/Tabs';

interface Props {
    address: string;
    type: 'object' | 'address';
}

export function TransactionsForAddress({ address, type }: Props) {
    const rpc = useRpcClient();

    const { data, isLoading, isError } = useQuery(
        ['transactions-for-address', address, type],
        async () => {
            const filters =
                type === 'object'
                    ? [{ InputObject: address }, { ChangedObject: address }]
                    : [{ ToAddress: address }, { FromAddress: address }];

            const results = await Promise.all(
                filters.map((filter) =>
                    rpc.queryTransactionBlocks({
                        filter,
                        order: 'descending',
                        limit: 100,
                        options: {
                            showEffects: true,
                            showBalanceChanges: true,
                            showInput: true,
                        },
                    })
                )
            );

            const inserted = new Map();
            const uniqueList: SuiTransactionBlockResponse[] = [];

            [...results[0].data, ...results[1].data]
                .sort(
                    (a, b) =>
                        Number(b.timestampMs ?? 0) - Number(a.timestampMs ?? 0)
                )
                .forEach((txb) => {
                    if (inserted.get(txb.digest)) return;
                    uniqueList.push(txb);
                    inserted.set(txb.digest, true);
                });

            return uniqueList;
        }
    );

    if (isLoading) {
        return (
            <div>
                <LoadingSpinner />
            </div>
        );
    }

    if (isError) {
        return (
            <Banner variant="error" fullWidth>
                Transactions could not be extracted on the following specified
                address: {address}
            </Banner>
        );
    }

    const tableData = genTableDataFromTxData(data);

    return (
        <div data-testid="tx">
            <TabGroup size="lg">
                <TabList>
                    <Tab>Transaction Blocks</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        <TableCard
                            data={tableData.data}
                            columns={tableData.columns}
                        />
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    );
}
