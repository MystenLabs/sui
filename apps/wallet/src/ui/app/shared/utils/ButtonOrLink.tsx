// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, forwardRef, type Ref } from 'react';
import { Link, type LinkProps } from 'react-router-dom';

export type ButtonOrLinkProps = Omit<
    Partial<LinkProps> & ComponentProps<'a'> & ComponentProps<'button'>,
    'ref'
>;

export const ButtonOrLink = forwardRef<
    HTMLAnchorElement | HTMLButtonElement,
    ButtonOrLinkProps
>(({ href, to, ...props }, ref) => {
    // External link:
    if (href && !props.disabled) {
        return (
            // eslint-disable-next-line jsx-a11y/anchor-has-content
            <a
                ref={ref as Ref<HTMLAnchorElement>}
                target="_blank"
                rel="noreferrer noopener"
                href={href}
                {...props}
            />
        );
    }
    // Internal router link:
    if (to && !props.disabled) {
        return <Link to={to} ref={ref as Ref<HTMLAnchorElement>} {...props} />;
    }
    return (
        <button
            {...props}
            type={props.type || 'button'}
            ref={ref as Ref<HTMLButtonElement>}
        />
    );
});

ButtonOrLink.displayName = 'ButtonOrLink';
