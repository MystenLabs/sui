// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, forwardRef } from 'react';
import { Link, type LinkProps } from 'react-router-dom';

export interface ButtonOrLinkProps
    extends Omit<
        Partial<LinkProps> & ComponentProps<'a'> & ComponentProps<'button'>,
        'ref'
    > {}

export default forwardRef<
    HTMLAnchorElement | HTMLButtonElement,
    ButtonOrLinkProps
>(({ href, to, ...props }, ref: any) => {
    // External link:
    if (href) {
        return (
            // eslint-disable-next-line jsx-a11y/anchor-has-content
            <a
                ref={ref}
                target="_blank"
                rel="noreferrer noopener"
                href={href}
                {...props}
            />
        );
    }

    // Internal router link:
    if (to) {
        return <Link to={to} ref={ref} {...props} />;
    }

    // NOTE: We set the default type to be "button" to avoid accidentally submitting forms.
    return <button {...props} type={props.type || 'button'} ref={ref} />;
});
