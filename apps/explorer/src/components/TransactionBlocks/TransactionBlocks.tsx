// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type PaginatedTransactionResponse } from '@mysten/sui.js';
import { type InfiniteData } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromTxData } from '../transactions/TxCardUtils';

import {
    DEFAULT_TRANSACTIONS_LIMIT,
    useGetTransactionBlocks,
} from '~/hooks/useGetTransactionBlocks';
import { Heading } from '~/ui/Heading';
import { Pagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { RadioGroup, RadioOption } from '~/ui/Radio';
import { TableCard } from '~/ui/TableCard';

type CurrentPageFilter = {
    from: number;
    to: number;
};

enum TRANSACTION_FILTERS {
    FROM = 'from',
    TO = 'to',
}

type TransactionBlocksProps = {
    address: string;
};

function TransactionBlocks({ address }: TransactionBlocksProps) {
    const [filterValue, setFilterValue] = useState<TRANSACTION_FILTERS>(
        TRANSACTION_FILTERS.FROM
    );
    const [currentPage, setCurrentPage] = useState<CurrentPageFilter>({
        from: 0,
        to: 0,
    });
    const {
        data,
        isLoading,
        isFetching,
        isFetchingNextPage,
        fetchNextPage,
        hasNextPage,
    } = useGetTransactionBlocks(
        address,
        filterValue === TRANSACTION_FILTERS.FROM
    );

    const setFilter = (value: TRANSACTION_FILTERS) => {
        setFilterValue(value);
    };

    const generateTableCard = (
        currentPage: CurrentPageFilter,
        filterValue: TRANSACTION_FILTERS,
        data?: InfiniteData<PaginatedTransactionResponse>
    ) => {
        if (!data) {
            return;
        }
        const cardData = genTableDataFromTxData(
            data?.pages[currentPage[filterValue]].data
        );
        return <TableCard data={cardData.data} columns={cardData.columns} />;
    };

    return (
        <div>
            <div className="flex items-center justify-between border-b border-gray-45 pb-5">
                <Heading color="gray-90" variant="heading4/semibold">
                    Transaction Blocks
                </Heading>
                <RadioGroup
                    className="flex"
                    ariaLabel="transaction filter"
                    value={filterValue}
                    onChange={setFilter}
                >
                    <RadioOption value="to" label="To Address" />
                    <RadioOption value="from" label="From Address" />
                </RadioGroup>
            </div>

            <div className="flex flex-col space-y-5 pt-5 text-left xl:pr-10">
                {isLoading || isFetching || isFetchingNextPage ? (
                    <PlaceholderTable
                        rowCount={DEFAULT_TRANSACTIONS_LIMIT}
                        rowHeight="16px"
                        colHeadings={[
                            'Digest',
                            'Sender',
                            'Txns',
                            'Gas',
                            'Time',
                        ]}
                        colWidths={['30%', '30%', '10%', '20%', '10%']}
                    />
                ) : (
                    <div data-testid="tx">
                        {generateTableCard(currentPage, filterValue, data)}
                    </div>
                )}

                {(hasNextPage || (data && data?.pages.length > 1)) && (
                    <Pagination
                        onNext={() => {
                            if (isLoading || isFetching) {
                                return;
                            }

                            // Make sure we are at the end before fetching another page
                            if (
                                data &&
                                currentPage[filterValue] ===
                                    data?.pages.length - 1 &&
                                !isLoading &&
                                !isFetching
                            ) {
                                fetchNextPage();
                            }
                            setCurrentPage({
                                ...currentPage,
                                [filterValue]: currentPage[filterValue] + 1,
                            });
                        }}
                        hasNext={Boolean(hasNextPage)}
                        hasPrev={currentPage[filterValue] !== 0}
                        onPrev={() =>
                            setCurrentPage({
                                ...currentPage,
                                [filterValue]: currentPage[filterValue] - 1,
                            })
                        }
                        onFirst={() =>
                            setCurrentPage({
                                ...currentPage,
                                [filterValue]: 1,
                            })
                        }
                    />
                )}
            </div>
        </div>
    );
}

export default TransactionBlocks;
