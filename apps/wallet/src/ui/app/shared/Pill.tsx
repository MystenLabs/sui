// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const styles = cva(
    [
        'inline-block outline-none transition no-underline bg-white py-1 px-2',
        'border border-solid border-sui-light rounded-20 cursor-pointer',
        'truncate leading-tight uppercase text-captionSmall font-semibold text-hero-dark',
        'hover:bg-sui-light focus:bg-sui-light',
        'active:bg-gray-45 active:text-steel-darker active:border-gray-45',
        'disabled:border-transparent disabled:text-gray-60 disabled:bg-white',
    ],
    {
        variants: {
            loading: {
                true: 'bg-white border-gray-45 text-steel disabled:border-gray-45 disabled:text-steel',
                false: '',
            },
        },
    }
);

export interface PillProps
    extends Omit<VariantProps<typeof styles>, 'loading'>,
        Omit<ButtonOrLinkProps, 'className'> {
    before?: ReactNode;
    after?: ReactNode;
    text?: ReactNode;
}

export const Pill = forwardRef(
    (
        { before, after, text, loading, ...otherProps }: PillProps,
        ref: Ref<HTMLAnchorElement | HTMLButtonElement>
    ) => (
        <ButtonOrLink
            className={styles({ loading })}
            {...otherProps}
            loading={loading}
            ref={ref}
        >
            {text}
        </ButtonOrLink>
    )
);

Pill.displayName = 'Pill';
