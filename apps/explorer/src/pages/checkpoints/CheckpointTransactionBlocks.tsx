// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { genTableDataFromTxData } from '~/components/transactions/TxCardUtils';
import { useGetTransactionBlocks } from '~/hooks/useGetTransactionBlocks';
import { Pagination, useCursorPagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';

const DEFAULT_TRANSACTIONS_LIMIT = 20;

export function CheckpointTransactionBlocks({ id }: { id: string }) {
    const [limit, setLimit] = useState(DEFAULT_TRANSACTIONS_LIMIT);
    const transactions = useGetTransactionBlocks(
        {
            Checkpoint: id,
        },
        limit
    );

    const { data, isFetching, pagination, isLoading } =
        useCursorPagination(transactions);

    const cardData = data ? genTableDataFromTxData(data.data) : undefined;

    return (
        <div className="flex flex-col space-y-5 text-left xl:pr-10">
            {isLoading || isFetching || !cardData ? (
                <PlaceholderTable
                    rowCount={20}
                    rowHeight="16px"
                    colHeadings={['Digest', 'Sender', 'Txns', 'Gas', 'Time']}
                    colWidths={['30%', '30%', '10%', '20%', '10%']}
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
                <Pagination {...pagination} />
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
            </div>
        </div>
    );
}
