// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const linkStyles = cva([], {
    variants: {
        variant: {
            text: 'text-body font-semibold text-steel-dark hover:text-steel-darker active:text-steel disabled:text-gray-60',
            mono: 'font-mono text-bodySmall font-medium text-hero-dark hover:text-hero-darkest break-all',
            textHeroDark:
                'text-pBody font-medium text-hero-dark hover:text-hero-darkest',
        },
        uppercase: {
            true: 'uppercase',
        },
        size: {
            md: '!text-body',
            sm: '!text-bodySmall',
            captionSmall: '!text-captionSmall',
        },
    },
    defaultVariants: {
        variant: 'text',
    },
});

export interface LinkProps
    extends ButtonOrLinkProps,
        VariantProps<typeof linkStyles> {
    before?: ReactNode;
    after?: ReactNode;
}

export function Link({
    variant,
    uppercase,
    size,
    before,
    after,
    children,
    ...props
}: LinkProps) {
    return (
        <ButtonOrLink
            className={linkStyles({ variant, size, uppercase })}
            {...props}
        >
            <div className="inline-flex flex-nowrap items-center gap-2">
                {before}
                {children}
                {after}
            </div>
        </ButtonOrLink>
    );
}
