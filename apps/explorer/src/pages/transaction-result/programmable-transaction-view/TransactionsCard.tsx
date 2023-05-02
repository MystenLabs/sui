// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransaction } from '@mysten/sui.js';

import { Transaction } from './Transaction';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

const DEFAULT_ITEMS_TO_SHOW = 5;

interface TransactionsCardProps {
    transactions: SuiTransaction[];
}

export function TransactionsCard({ transactions }: TransactionsCardProps) {
    const defaultOpen = transactions.length <= DEFAULT_ITEMS_TO_SHOW;

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
        <TransactionBlockCard collapsible title="Transactions">
            <ExpandableList
                items={expandableItems}
                defaultItemsToShow={DEFAULT_ITEMS_TO_SHOW}
                itemsLabel="Transactions"
            >
                <div className="flex max-h-[300px] flex-col gap-6 overflow-y-auto">
                    <ExpandableListItems />
                </div>

                {expandableItems.length > DEFAULT_ITEMS_TO_SHOW && (
                    <div className="mt-6">
                        <ExpandableListControl />
                    </div>
                )}
            </ExpandableList>
        </TransactionBlockCard>
    );
}
