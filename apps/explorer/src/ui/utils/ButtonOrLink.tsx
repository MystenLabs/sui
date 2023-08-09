// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, forwardRef } from 'react';

import { LinkWithQuery, type LinkProps } from './LinkWithQuery';

export interface ButtonOrLinkProps
	extends Omit<Partial<LinkProps> & ComponentProps<'a'> & ComponentProps<'button'>, 'ref'> {}

export const ButtonOrLink = forwardRef<HTMLAnchorElement | HTMLButtonElement, ButtonOrLinkProps>(
	({ href, to, ...props }, ref: any) => {
		// External link:
		if (href) {
			return (
				// eslint-disable-next-line jsx-a11y/anchor-has-content
				<a ref={ref} target="_blank" rel="noreferrer noopener" href={href} {...props} />
			);
		}

		// Internal router link:
		if (to) {
			return <LinkWithQuery to={to} ref={ref} {...props} />;
		}

		// We set the default type to be "button" to avoid accidentally submitting forms.
		// eslint-disable-next-line react/button-has-type
		return <button {...props} type={props.type || 'button'} ref={ref} />;
	},
);
