// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const headingStyles = cva(['font-sans'], {
    variants: {
        variant: {
            h1: 'text-h1 leading-80',
            h2: 'text-h2 leading-80',
            h3: 'text-h3 leading-100',
            h4: 'text-h4 leading-100',
            h5: 'text-h5 leading-100',
            h6: 'text-h6 leading-100',
        },
        weight: {
            medium: 'font-medium',
            semibold: 'font-semibold',
            bold: 'font-bold',
        },
    },
    defaultVariants: {
        variant: 'h1',
        weight: 'semibold',
    },
});

export interface HeadingProps extends VariantProps<typeof headingStyles> {
    /**
     * The HTML element that will be rendered.
     * By default, we render a "div" in order to separate presentational styles from semantic markup.
     */
    tag?: 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6' | 'div';
    children: ReactNode;
}

export function Heading({
    tag: Tag = 'div',
    children,
    ...styleProps
}: HeadingProps) {
    return <Tag className={headingStyles(styleProps)}>{children}</Tag>;
}
