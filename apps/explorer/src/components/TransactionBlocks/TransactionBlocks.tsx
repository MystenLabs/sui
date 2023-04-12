// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRef, useState, useEffect } from "react";

import { useGetTransactionBlocks } from "~/hooks/useGetTransactionBlocks";
import { useOnScreen } from "~/hooks/useOnScreen";
import { Button } from "~/ui/Button";
import { Heading } from "~/ui/Heading";
import { LoadingSpinner } from "~/ui/LoadingSpinner";
import { TableCard } from "~/ui/TableCard";
import {genTableDataFromTxData} from '../transactions/TxCardUtils'

type TransactionBlocksProps = {
    address: string;
};

function TransactionBlocks({ address }: TransactionBlocksProps) {
    const [isFrom, setIsFrom] = useState(false)
    const { data, isLoading, isFetching, fetchNextPage, hasNextPage } =
    useGetTransactionBlocks(address, isFrom);
    
    const containerRef = useRef(null);
    const { isIntersecting } = useOnScreen(containerRef);
    const toggleIsFrom = () => {
        setIsFrom(!isFrom)
    }

    // console.log(data, isFrom)
    // const tableData = data.pages[0].data);
    // const flatData = data?.pages?.map(page => {
        // return page.data
    // }) || []

    // const tableData = flatData && genTableDataFromTxData(flatData)

    // console.log(flatData)

    const isSpinnerVisible = hasNextPage || isLoading || isFetching;

    useEffect(() => {
        if (isIntersecting && hasNextPage && !isFetching) {
            fetchNextPage();
        }
    }, [isIntersecting, hasNextPage, isFetching, fetchNextPage]);

    return <div>
        <div className="flex justify-between items-center border-b border-gray-45 pb-5">
            <Heading color="gray-90" variant="heading4/semibold">
                Transaction Blocks
            </Heading>
            <div className="flex gap-2">
                <Button disabled={isFrom} onClick={toggleIsFrom} variant="outline">TO ADDRESS</Button>
                <Button disabled={!isFrom} onClick={toggleIsFrom} variant="outline">FROM ADDRESS</Button>
            </div>
            
        </div>
        {isLoading || isFetching ? <LoadingSpinner /> : <div>
        <div data-testid="tx">
            {data?.pages.map(page => {
                const cardData = genTableDataFromTxData(page.data)
                return <TableCard data={cardData.data} columns={cardData.columns} />
            })}
        </div>
        {isSpinnerVisible && (
                <div ref={containerRef}>
                    <LoadingSpinner />
                </div>
            )}
            </div>}
    </div>
}

export default TransactionBlocks


// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { useRpcClient } from '@mysten/core';
// import { type SuiTransactionBlockResponse } from '@mysten/sui.js';
// import { useQuery } from '@tanstack/react-query';

// import { genTableDataFromTxData } from './TxCardUtils';

// import { Banner } from '~/ui/Banner';
// import { LoadingSpinner } from '~/ui/LoadingSpinner';
// import { TableCard } from '~/ui/TableCard';

// interface Props {
//     address: string;
//     type: 'object' | 'address';
// }

// export function TransactionsForAddress({ address, type }: Props) {
//     const rpc = useRpcClient();

//     const { data, isLoading, isError } = useQuery(
//         ['transactions-for-address', address, type],
//         async () => {
//             const filters =
//                 type === 'object'
//                     ? [{ InputObject: address }, { ChangedObject: address }]
//                     : [{ ToAddress: address }, { FromAddress: address }];
            
//             const results = await Promise.all(
//                 filters.map((filter) =>
//                     rpc.queryTransactionBlocks({
//                         filter,
//                         order: 'descending',
//                         limit: 100,
//                         options: {
                            
//                             showEffects: true,
//                             showBalanceChanges: true,
//                             showInput: true,
//                         },
//                     })
//                 )
//             );

//             const inserted = new Map();
//             const uniqueList: SuiTransactionBlockResponse[] = [];

//             [...results[0].data, ...results[1].data]
//                 .sort((a, b) => +(b.timestampMs ?? 0) - +(a.timestampMs ?? 0))
//                 .forEach((txb) => {
//                     if (inserted.get(txb.digest)) return;
//                     uniqueList.push(txb);
//                     inserted.set(txb.digest, true);
//                 });

//             return uniqueList;
//         }
//     );

//     if (isLoading) {
//         return (
//             <div>
//                 <LoadingSpinner />
//             </div>
//         );
//     }

//     if (isError) {
//         return (
//             <Banner variant="error" fullWidth>
//                 Transactions could not be extracted on the following specified
//                 address: {address}
//             </Banner>
//         );
//     }

//     const tableData = genTableDataFromTxData(data);

//     return (
//         <div data-testid="tx">
//             <TableCard data={tableData.data} columns={tableData.columns} />
//         </div>
//     );
// }
