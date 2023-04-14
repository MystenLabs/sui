// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type PaginatedTransactionResponse } from '@mysten/sui.js';
import { type InfiniteData } from '@tanstack/react-query';
import { useState } from 'react';

import { genTableDataFromTxData } from '../transactions/TxCardUtils';

import {
    DEFAULT_TRANSACTIONS_LIMIT,
    useGetTransactionBlocksForAddress,
} from '~/hooks/useGetTransactionBlocksForAddress';
import { Heading } from '~/ui/Heading';
import { Pagination } from '~/ui/Pagination';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';

type TransactionBlocksForAddressProps = {
    address: string;
};

function TransactionBlocksForAddress({
    address,
}: TransactionBlocksForAddressProps) {
    const [currentPage, setCurrentPage] = useState(0);
    const {
        data,
        isLoading,
        isFetching,
        isFetchingNextPage,
        fetchNextPage,
        hasNextPage,
    } = useGetTransactionBlocksForAddress(address);

    const generateTableCard = (
        currentPage: number,
        data?: InfiniteData<PaginatedTransactionResponse>
    ) => {
        if (!data) {
            return;
        }
        const cardData = genTableDataFromTxData(data?.pages[currentPage].data);
        return <TableCard data={cardData.data} columns={cardData.columns} />;
    };

    return (
        <div data-testid="tx">
            <div className="flex items-center justify-between border-b border-gray-45 pb-5">
                <Heading color="gray-90" variant="heading4/semibold">
                    Transaction Blocks
                </Heading>
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
                    <div>{generateTableCard(currentPage, data)}</div>
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
                                currentPage === data?.pages.length - 1 &&
                                !isLoading &&
                                !isFetching
                            ) {
                                fetchNextPage();
                            }
                            setCurrentPage(currentPage + 1);
                        }}
                        hasNext={Boolean(hasNextPage)}
                        hasPrev={currentPage !== 0}
                        onPrev={() => setCurrentPage(currentPage - 1)}
                        onFirst={() => setCurrentPage(1)}
                    />
                )}
            </div>
        </div>
    );
}

export default TransactionBlocksForAddress;
