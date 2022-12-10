// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { memo } from 'react';

import type { ReactNode } from 'react';

const cardHeaderStyle = cva(
    [
        'bg-gray-40 min-h-[30px] flex justify-center items-center rounded-t-2xl divide-x divide-solid divide-gray-45 divide-y-0 w-full',
    ],
    {
        variants: {
            background: {
                grey: 'bg-gray-40',
                transparent: 'bg-transparent',
            },
        },
        defaultVariants: {
            background: 'grey',
        },
    }
);

export interface CardHeaderProps extends VariantProps<typeof cardHeaderStyle> {
    children: ReactNode | ReactNode[];
}

function CardHeader({ children, ...styleProps }: CardHeaderProps) {
    return <div className={cardHeaderStyle(styleProps)}>{children}</div>;
}

export default memo(CardHeader);
