// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const headingStyles = cva(
    [
        // TODO: Remove when CSS reset is applied.
        'my-0',
    ],
    {
        variants: {
            /**
             * The size of the heading that will be displayed.
             * The variant is expressed in the desktop size, and will automatically adjust for mobile.
             * Set the `fixed` property to disable responsive sizing.
             */
            variant: {
                heading1: 'text-heading1',
                heading2: 'md:text-heading2 text-heading3',
                heading3: 'text-heading3',
                heading4: 'md:text-heading4 text-heading6',
                heading5: 'text-heading5',
                heading6: 'text-heading6',
            },
            color: {
                'gray-100': 'text-gray-100',
                'gray-90': 'text-gray-90',
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
                'success-dark': 'text-success-dark',
                'hero-dark': 'text-hero-dark',
            },
            weight: {
                medium: 'font-medium',
                semibold: 'font-semibold',
                bold: 'font-bold',
            },
            mono: {
                true: 'font-mono',
                false: 'font-sans',
            },
            leading: {
                none: '!leading-none',
                tight: 'leading-tight',
            },
            truncate: {
                true: 'truncate',
                false: '',
            },
            /** Fix the header size, and disable responsive sizing of the heading. */
            fixed: { true: '', false: '' },
        },
        defaultVariants: {
            variant: 'heading1',
            weight: 'semibold',
            leading: 'tight',
        },
        // Use the empty `fixed` variant to force text size to a set value:
        compoundVariants: [
            { fixed: true, variant: 'heading1', class: '!text-heading1' },
            { fixed: true, variant: 'heading2', class: '!text-heading2' },
            { fixed: true, variant: 'heading3', class: '!text-heading3' },
            { fixed: true, variant: 'heading4', class: '!text-heading4' },
            { fixed: true, variant: 'heading5', class: '!text-heading5' },
            { fixed: true, variant: 'heading6', class: '!text-heading6' },
        ],
    }
);

export interface HeadingProps extends VariantProps<typeof headingStyles> {
    /**
     * The HTML element that will be rendered.
     * By default, we render a "div" in order to separate presentational styles from semantic markup.
     */
    as?: 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6' | 'div';
    children: ReactNode;
}

export function Heading({
    as: Tag = 'div',
    children,
    ...styleProps
}: HeadingProps) {
    return <Tag className={headingStyles(styleProps)}>{children}</Tag>;
}
