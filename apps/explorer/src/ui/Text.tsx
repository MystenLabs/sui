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
            'grey-100': 'text-sui-grey-100',
            'grey-90': 'text-sui-grey-90',
            'grey-75': 'text-sui-grey-75',
            'grey-70': 'text-sui-grey-70',
            'grey-65': 'text-sui-grey-65',
            'sui-dark': 'text-sui-dark',
            sui: 'text-sui',
            'sui-light': 'text-sui-light',
            'sui-steel': 'text-sui-steel',
            'sui-steel-dark': 'text-sui-steel-dark',
            'sui-steel-darker': 'text-sui-steel-darker',
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
