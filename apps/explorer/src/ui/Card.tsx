// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const cardStyles = cva(null, {
    variants: {
        bg: {
            default: 'bg-gray-40',
            highlight: 'bg-success-light',
            lightBlue: 'bg-sui/10',
            white: 'bg-white border border-steel',
        },
        rounded: {
            lg: 'rounded-lg',
            xl: 'rounded-xl',
        },
        spacing: {
            none: '',
            sm: 'px-5 py-4',
            md: 'p-5',
            lg: 'p-8',
        },
    },
    defaultVariants: {
        bg: 'default',
        spacing: 'md',
        rounded: 'lg',
    },
});

export interface CardProps extends VariantProps<typeof cardStyles> {
    children?: ReactNode;
}

export function Card({ spacing, rounded, bg, children }: CardProps) {
    return (
        <div className={cardStyles({ spacing, rounded, bg })}>{children}</div>
    );
}
