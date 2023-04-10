// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { parseVariant, type SizeAndWeightVariant } from './utils/sizeAndWeight';

const textStyles = cva(['break-words'], {
    variants: {
        size: {
            body: 'text-body',
            bodySmall: 'text-bodySmall',
            subtitle: 'text-subtitle',
            subtitleSmall: 'text-subtitleSmall',
            subtitleSmallExtra: 'text-subtitleSmallExtra',
            caption: 'uppercase text-caption',
            captionSmall: 'uppercase text-captionSmall',
            p1: 'text-p1',
            p2: 'text-p2',
            p3: 'text-p3',
            p4: 'text-p4',
        },
        weight: {
            medium: 'font-medium',
            normal: 'font-normal',
            semibold: 'font-semibold',
            bold: 'font-bold',
        },
        color: {
            white: 'text-white',
            'gray-100': 'text-gray-100',
            'gray-90': 'text-gray-90',
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
            'hero-dark': 'text-hero-dark',
            'success-dark': 'text-success-dark',
            issue: 'text-issue',
        },
        uppercase: { true: 'uppercase' },
        italic: {
            true: 'italic',
            false: '',
        },
        mono: {
            true: 'font-mono',
            false: 'font-sans',
        },
        truncate: {
            true: 'truncate',
        },
    },
});

type TextStylesProps = VariantProps<typeof textStyles>;

export interface TextProps extends Omit<TextStylesProps, 'size' | 'weight'> {
    variant: SizeAndWeightVariant<TextStylesProps>;
    children: ReactNode;
}

export function Text({
    children,
    variant = 'body/medium',
    ...styleProps
}: TextProps) {
    const [size, weight] = parseVariant<TextStylesProps>(variant);

    return (
        <div className={textStyles({ size, weight, ...styleProps })}>
            {children}
        </div>
    );
}
