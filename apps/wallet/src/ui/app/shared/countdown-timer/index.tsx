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
}

export function CountDownTimer({
    timestamp,
    label,
    ...styles
}: CountDownTimerProps) {
    const timeAgo = useTimeAgo(timestamp, false, true);

    return (
        <div className={timeStyle(styles)}>
            {label} {timeAgo}
        </div>
    );
}
