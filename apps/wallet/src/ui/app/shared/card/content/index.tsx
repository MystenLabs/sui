// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { memo } from 'react';

import type { ReactNode } from 'react';

const cardContentStyle = cva(
    ['divide-x flex divide-solid divide-gray-45 divide-y-0'],
    {
        variants: {
            padding: {
                true: 'p-4',
            },
            col: {
                true: 'flex-col',
            },
            gap: {
                true: 'gap-3.5',
            },
            colored: {
                true: 'bg-sui/10',
            },
        },
        defaultVariants: {
            padding: false,
            col: false,
            gap: false,
            colored: false,
        },
    }
);

export interface CardContentProps
    extends VariantProps<typeof cardContentStyle> {
    children: ReactNode;
}

function CardContent({ children, ...styleProps }: CardContentProps) {
    return <div className={cardContentStyle(styleProps)}>{children}</div>;
}

export default memo(CardContent);
