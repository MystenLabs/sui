// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import { TransactionBlockCard } from '~/ui/TransactionBlockCard';

interface ProgrammableTxnBlockCardProps {
    items: ReactNode[];
    itemsLabel: string;
    defaultItemsToShow: number;
}

export function ProgrammableTxnBlockCard({
    items,
    itemsLabel,
    defaultItemsToShow,
}: ProgrammableTxnBlockCardProps) {
    if (!items?.length) {
        return null;
    }

    return (
        <TransactionBlockCard collapsible title={itemsLabel}>
            <ExpandableList
                items={items}
                defaultItemsToShow={defaultItemsToShow}
                itemsLabel={itemsLabel}
            >
                <div className="flex max-h-[300px] flex-col gap-6 overflow-y-auto">
                    <ExpandableListItems />
                </div>

                {items.length > defaultItemsToShow && (
                    <div className="mt-6">
                        <ExpandableListControl />
                    </div>
                )}
            </ExpandableList>
        </TransactionBlockCard>
    );
}
