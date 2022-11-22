// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const textStyles = cva([], {
    variants: {
        weight: {
            medium: 'font-medium',
            semibold: 'font-semibold',
            bold: 'font-bold',
        },
        variant: {
            body: 'text-body',
            bodySmall: 'text-bodySmall',
            subtitle: 'text-subtitle',
            subtitleSmall: 'text-subtitleSmall',
            subtitleSmallExtra: 'text-subtitleSmallExtra',
            caption: 'uppercase text-caption',
            captionSmall: 'uppercase text-captionSmall ',
        },
        color: {
            'gray-100': 'text-gray-100',
            'gray-90': 'text-gray-90',
            'gray-75': 'text-gray-75',
            'gray-70': 'text-gray-70',
            'gray-65': 'text-gray-65',
            'sui-dark': 'text-sui-dark',
            sui: 'text-sui',
            'sui-light': 'text-sui-light',
            steel: 'text-steel',
            'steel-dark': 'text-steel-dark',
            'steel-darker': 'text-steel-darker',
        },
        italic: {
            true: 'italic',
            false: '',
        },
        mono: {
            true: 'font-mono',
            false: 'font-sans',
        },
    },
    defaultVariants: {
        weight: 'medium',
        variant: 'body',
    },
});

export interface TextProps extends VariantProps<typeof textStyles> {
    children: ReactNode;
}

export function Text({ children, ...styleProps }: TextProps) {
    return <div className={textStyles(styleProps)}>{children}</div>;
}
