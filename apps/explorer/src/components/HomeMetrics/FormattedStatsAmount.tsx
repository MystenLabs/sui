// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { formatAmountParts } from '@mysten/core';

import { Stats, type StatsProps } from '~/ui/Stats';

// Simple wrapper around stats to avoid text wrapping:
export function StatsWrapper(props: StatsProps) {
    return (
        <div className="flex-shrink-0">
            <Stats {...props} />
        </div>
    );
}

export function FormattedStatsAmount({
    amount,
    ...props
}: Omit<StatsProps, 'children'> & {
    amount?: string | number | bigint;
}) {
    const [formattedAmount, postfix] = formatAmountParts(amount);

    return (
        <StatsWrapper {...props} postfix={postfix}>
            {formattedAmount}
        </StatsWrapper>
    );
}
