// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, forwardRef, useMemo } from 'react';

import { LinkWithQuery, type LinkProps } from './LinkWithQuery';

export interface ButtonOrLinkProps
    extends Omit<
        Partial<LinkProps> & ComponentProps<'a'> & ComponentProps<'button'>,
        'ref'
    > {
    prefixIcon?: JSX.Element;
    postfixIcon?: JSX.Element;
}

export const ButtonOrLink = forwardRef<
    HTMLAnchorElement | HTMLButtonElement,
    ButtonOrLinkProps
>(({ href, to, prefixIcon, postfixIcon, children, ...props }, ref: any) => {
    const childrenContent = useMemo(() => {
        if (prefixIcon || postfixIcon) {
            return (
                <div className="inline-flex flex-nowrap gap-2">
                    {prefixIcon}
                    {children}
                    {postfixIcon}
                </div>
            );
        }

        return children;
    }, [children, postfixIcon, prefixIcon]);

    if (href) {
        return (
            // eslint-disable-next-line jsx-a11y/anchor-has-content
            <a
                ref={ref}
                target="_blank"
                rel="noreferrer noopener"
                href={href}
                {...props}
            >
                {childrenContent}
            </a>
        );
    }

    // Internal router link:
    if (to) {
        return (
            <LinkWithQuery to={to} ref={ref} {...props}>
                {childrenContent}
            </LinkWithQuery>
        );
    }

    return (
        // We set the default type to be "button" to avoid accidentally submitting forms.
        // eslint-disable-next-line react/button-has-type
        <button {...props} type={props.type || 'button'} ref={ref}>
            {childrenContent}
        </button>
    );
});
