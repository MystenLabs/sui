// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const linkStyles = cva(
    [
        // TODO: Remove when CSS reset is applied.
        'cursor-pointer no-underline bg-transparent p-0 border-none',
    ],
    {
        variants: {
            variant: {
                text: 'text-body font-semibold text-sui-grey-75 hover:text-sui-grey-90 active:text-sui-grey-100',
                mono: 'font-mono text-bodySmall font-medium text-sui-dark',
            },
            uppercase: {
                true: 'uppercase',
            },
        },
        defaultVariants: {
            variant: 'text',
        },
    }
);

export interface LinkProps
    extends ButtonOrLinkProps,
        VariantProps<typeof linkStyles> {}

export function Link({ variant, ...props }: LinkProps) {
    return <ButtonOrLink className={linkStyles({ variant })} {...props} />;
}
