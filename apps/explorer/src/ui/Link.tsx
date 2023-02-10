// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const linkStyles = cva([], {
    variants: {
        variant: {
            text: 'text-body font-semibold text-steel-dark hover:text-steel-darker active:text-steel disabled:text-gray-60',
            mono: 'font-mono text-bodySmall font-medium text-sui-dark break-all',
            footer: 'text',
        },
        uppercase: {
            true: 'uppercase',
        },
        size: {
            md: '!text-body',
            sm: '!text-bodySmall',
        },
    },
    defaultVariants: {
        variant: 'text',
    },
});

export interface LinkProps
    extends ButtonOrLinkProps,
        VariantProps<typeof linkStyles> {}

export function Link({ variant, size, ...props }: LinkProps) {
    return (
        <ButtonOrLink className={linkStyles({ variant, size })} {...props} />
    );
}
