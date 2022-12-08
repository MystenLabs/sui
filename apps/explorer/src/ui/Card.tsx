// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const cardStyles = cva('bg-gray-40', {
    variants: {
        spacing: {
            sm: 'px-5 py-4 rounded-lg',
            md: 'p-5 rounded-xl',
            lg: 'p-8 rounded-xl',
        },
    },
    defaultVariants: {
        spacing: 'md',
    },
});

export interface CardProps extends VariantProps<typeof cardStyles> {
    children: ReactNode;
}

export function Card({ spacing, children }: CardProps) {
    return <div className={cardStyles({ spacing })}>{children}</div>;
}
