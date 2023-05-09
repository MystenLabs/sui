// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronDown12 } from '@mysten/icons';
import clsx from 'classnames';
import { type ReactNode, useMemo, useState } from 'react';

import { Link } from './Link';
import { Text } from './text';

interface ExpandableListProps {
    items: ReactNode[];
    defaultItemsToShow: number;
}

export function ExpandableList({
    items,
    defaultItemsToShow,
}: ExpandableListProps) {
    const [showAll, setShowAll] = useState(false);

    const itemsDisplayed = useMemo(
        () => (showAll ? items : items?.slice(0, defaultItemsToShow)),
        [showAll, items, defaultItemsToShow]
    );

    const handleShowAllClick = () =>
        setShowAll((prevShowAll: boolean) => !prevShowAll);

    return (
        <>
            {itemsDisplayed.map((item, index) => (
                <div key={index}>{item}</div>
            ))}
            {items.length > defaultItemsToShow && (
                <div className="flex cursor-pointer items-center">
                    <Link
                        onClick={handleShowAllClick}
                        after={
                            <ChevronDown12
                                height={12}
                                width={12}
                                className={clsx(
                                    'text-steel hover:text-steel-dark',
                                    {
                                        'rotate-180': showAll,
                                    }
                                )}
                            />
                        }
                    >
                        <Text variant="bodySmall">
                            {showAll ? 'Show Less' : 'Show All'}
                        </Text>
                    </Link>
                </div>
            )}
        </>
    );
}
