// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const styles = cva([
    'flex flex-nowrap items-center justify-center outline-none gap-1 w-full',
    'no-underline bg-transparent p-0 border-none',
    'text-bodySmall font-semibold text-steel-dark',
    'hover:text-steel-darker focus:text-steel-darker',
    'active:opacity-70',
    'disabled:opacity-40 disabled:text-steel-dark',
    'cursor-pointer group',
]);

const iconStyles = cva([
    'flex text-steel',
    'group-hover:text-steel-darker group-focus:text-steel-darker group-disabled:text-steel-dark',
]);

interface LinkProps
    extends VariantProps<typeof styles>,
        VariantProps<typeof iconStyles>,
        Omit<ButtonOrLinkProps, 'className'> {
    before?: ReactNode;
    after?: ReactNode;
    text?: ReactNode;
}

export const Link = forwardRef(
    (
        { before, after, text, ...otherProps }: LinkProps,
        ref: Ref<HTMLAnchorElement | HTMLButtonElement>
    ) => (
        <ButtonOrLink className={styles()} {...otherProps} ref={ref}>
            {before ? <div className={iconStyles()}>{before}</div> : null}
            {text ? (
                <div className={'truncate leading-tight'}>{text}</div>
            ) : null}
            {after ? <div className={iconStyles()}>{after}</div> : null}
        </ButtonOrLink>
    )
);

Link.displayName = 'Link';
