// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransaction } from '@mysten/sui.js';

import { Transaction } from './Transaction';

import { ProgrammableTxnBlockCard } from '~/components/transactions/ProgTxnBlockCard';
import { TransactionBlockCardSection } from '~/ui/TransactionBlockCard';

const DEFAULT_ITEMS_TO_SHOW = 5;

interface TransactionsCardProps {
    transactions: SuiTransaction[];
}

export function TransactionsCard({ transactions }: TransactionsCardProps) {
    const defaultOpen = transactions.length < DEFAULT_ITEMS_TO_SHOW;

    if (!transactions?.length) {
        return null;
    }

    const expandableItems = transactions.map((transaction, index) => {
        const [[type, data]] = Object.entries(transaction);

        return (
            <TransactionBlockCardSection
                key={index}
                title={type}
                defaultOpen={defaultOpen}
            >
                <Transaction key={index} type={type} data={data} />
            </TransactionBlockCardSection>
        );
    });

    return (
        <ProgrammableTxnBlockCard
            items={expandableItems}
            itemsLabel="Transactions"
            defaultItemsToShow={DEFAULT_ITEMS_TO_SHOW}
            noExpandableList={defaultOpen}
        />
    );
}
