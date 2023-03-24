// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import { genTableDataFromTxData } from './TxCardUtils';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { TableCard } from '~/ui/TableCard';

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
                    rpc.queryTransactions({
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

            return [...results[0].data, ...results[1].data].sort(
                (a, b) => (b.timestampMs ?? 0) - (a.timestampMs ?? 0)
            );
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
            <TableCard data={tableData.data} columns={tableData.columns} />
        </div>
    );
}
