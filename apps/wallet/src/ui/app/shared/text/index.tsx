// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const textStyles = cva([], {
    variants: {
        weight: {
            normal: 'font-normal',
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
            heading4: 'text-heading4',
            p1: 'text-p1',
            p2: 'text-p2',
            p3: 'text-p3',
        },
        color: {
            'gray-100': 'text-gray-100',
            'gray-90': 'text-gray-90',
            'gray-85': 'text-gray-85',
            'gray-80': 'text-gray-80',
            'gray-75': 'text-gray-75',
            'gray-70': 'text-gray-70',
            'gray-65': 'text-gray-65',
            'gray-60': 'text-gray-60',
            'sui-dark': 'text-sui-dark',
            sui: 'text-sui',
            'sui-light': 'text-sui-light',
            steel: 'text-steel',
            'steel-dark': 'text-steel-dark',
            'steel-darker': 'text-steel-darker',
            hero: 'text-hero',
            'hero-dark': 'text-hero-dark',
            'success-dark': 'text-success-dark',
        },
        italic: {
            true: 'italic',
            false: '',
        },
        truncate: {
            true: 'truncate',
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
