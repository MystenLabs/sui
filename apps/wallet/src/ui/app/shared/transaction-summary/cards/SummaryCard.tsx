// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ChevronDown16, ChevronRight16 } from '@mysten/icons';
import clsx from 'classnames';
import { useState, type ReactNode } from 'react';

import { Text } from '../../../shared/text';

type SummaryCardProps = {
    header: ReactNode;
    children: ReactNode;
    badge?: ReactNode;
    initialExpanded?: boolean;
};

export function SummaryCard({
    children,
    header,
    badge,
    initialExpanded = false,
}: SummaryCardProps) {
    const [expanded, setExpanded] = useState(initialExpanded);

    return (
        <div
            className={clsx(
                'border border-solid rounded-2xl overflow-hidden',
                expanded ? 'border-gray-45' : 'border-gray-40'
            )}
        >
            <button
                onClick={() => setExpanded((expanded) => !expanded)}
                className="bg-gray-40 px-4 py-2 flex items-center w-full cursor-pointer border-none relative gap-1.5 text-left"
            >
                <div className="flex-1">
                    <Text
                        variant="captionSmall"
                        weight="semibold"
                        color="steel-darker"
                    >
                        {header}
                    </Text>
                </div>

                {badge}

                <div className="text-steel flex items-center justify-center">
                    {expanded ? <ChevronDown16 /> : <ChevronRight16 />}
                </div>
            </button>
            {expanded && <div className="px-4 py-3">{children}</div>}
        </div>
    );
}
