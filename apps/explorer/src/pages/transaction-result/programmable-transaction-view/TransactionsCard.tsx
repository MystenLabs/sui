// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransaction } from '@mysten/sui.js';

import { Transaction } from './Transaction';

import { ExpandableList } from '~/ui/ExpandableList';
import { TransactionCard, TransactionCardSection } from '~/ui/TransactionCard';

interface TransactionsCardProps {
    transactions: SuiTransaction[];
}

export function TransactionsCard({ transactions }: TransactionsCardProps) {
    const collapsedThreshold = transactions.length > 5;
    const defaultItemsToShow = collapsedThreshold ? 5 : transactions?.length;

    if (!transactions?.length) {
        return null;
    }

    const expandableItems = transactions.map((transaction, index) => {
        const [[type, data]] = Object.entries(transaction);

        return (
            <TransactionCardSection
                key={index}
                title={type}
                collapsedOnLoad={collapsedThreshold}
            >
                <Transaction key={index} type={type} data={data} />
            </TransactionCardSection>
        );
    });

    return (
        <TransactionCard collapsible title="Transactions">
            <ExpandableList
                items={expandableItems}
                defaultItemsToShow={defaultItemsToShow}
                itemsLabel="Transactions"
            />
        </TransactionCard>
    );
}
