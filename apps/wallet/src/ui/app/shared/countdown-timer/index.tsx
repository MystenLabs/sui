// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTimeAgo } from '@mysten/core';
import { cva, type VariantProps } from 'class-variance-authority';

const timeStyle = cva([], {
    variants: {
        variant: {
            body: 'text-body',
            bodySmall: 'text-bodySmall',
        },
        color: {
            'steel-dark': 'text-steel-dark',
            'steel-darker': 'text-steel-darker',
        },
        weight: {
            medium: 'font-medium',
            semibold: 'font-semibold',
        },
    },
    defaultVariants: {
        variant: 'body',
        color: 'steel-dark',
        weight: 'semibold',
    },
});

export interface CountDownTimerProps extends VariantProps<typeof timeStyle> {
    timestamp: number | undefined;
    label?: string;
    endLabel?: string;
}

export function CountDownTimer({
    timestamp,
    label,
    endLabel = 'now',
    ...styles
}: CountDownTimerProps) {
    const timeAgo = useTimeAgo(timestamp, false, true, endLabel);

    return (
        <div className={timeStyle(styles)}>
            {timeAgo === endLabel ? '' : label} {timeAgo}
        </div>
    );
}
