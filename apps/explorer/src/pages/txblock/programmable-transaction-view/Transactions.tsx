// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiTransaction } from '@mysten/sui.js';

import { Transaction } from './Transaction';

import { ExpandableList } from '~/ui/ExpandableList';
import { TableHeader } from '~/ui/TableHeader';

interface Props {
    transactions: SuiTransaction[];
}

export function Transactions({ transactions }: Props) {
    if (!transactions?.length) {
        return null;
    }

    return (
        <>
            <TableHeader>Transactions</TableHeader>
            <ul className="flex flex-col gap-8">
                <ExpandableList
                    items={transactions.map((transaction, index) => {
                        const [[type, data]] = Object.entries(transaction);

                        return (
                            <li key={index}>
                                <Transaction type={type} data={data} />
                            </li>
                        );
                    })}
                    defaultItemsToShow={10}
                />
            </ul>
        </>
    );
}
