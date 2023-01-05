// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const textStyles = cva([], {
    variants: {
        size: {
            body: 'text-body',
            bodySmall: 'text-bodySmall',
            subtitle: 'text-subtitle',
            subtitleSmall: 'text-subtitleSmall',
            subtitleSmallExtra: 'text-subtitleSmallExtra',
            caption: 'uppercase text-caption',
            captionSmall: 'uppercase text-captionSmall ',
        },
        weight: {
            medium: 'font-medium',
            semibold: 'font-semibold',
            bold: 'font-bold',
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
});

type TextStylesProps = VariantProps<typeof textStyles>;
type Variant = `${TextStylesProps['size']}/${TextStylesProps['weight']}`;

export interface TextProps extends Omit<TextStylesProps, 'size' | 'weight'> {
    variant: Variant;
    children: ReactNode;
}

export function Text({
    children,
    variant = 'body/medium',
    ...styleProps
}: TextProps) {
    const [size, weight] = variant.split('/') as [
        TextStylesProps['size'],
        TextStylesProps['weight']
    ];

    return (
        <div className={textStyles({ size, weight, ...styleProps })}>
            {children}
        </div>
    );
}
