// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const headingStyles = cva(
    [
        'font-sans',
        // TODO: Remove when CSS reset is applied.
        'my-0',
    ],
    {
        variants: {
            variant: {
                heading1: 'text-heading1',
                heading2: 'text-heading2',
                heading3: 'text-heading3',
                heading4: 'text-heading4',
                heading5: 'text-heading5',
                heading6: 'text-heading6',
            },
            weight: {
                medium: 'font-medium',
                semibold: 'font-semibold',
                bold: 'font-bold',
            },
        },
        defaultVariants: {
            variant: 'heading1',
            weight: 'semibold',
        },
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
