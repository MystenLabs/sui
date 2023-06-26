// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, forwardRef, type Ref } from 'react';
import { Link, type LinkProps } from 'react-router-dom';

import LoadingIndicator from '../../components/loading/LoadingIndicator';

export interface ButtonOrLinkProps
	extends Omit<Partial<LinkProps> & ComponentProps<'a'> & ComponentProps<'button'>, 'ref'> {
	loading?: boolean;
}

export const ButtonOrLink = forwardRef<HTMLAnchorElement | HTMLButtonElement, ButtonOrLinkProps>(
	({ href, to, disabled = false, loading = false, children, ...props }, ref) => {
		const isDisabled = disabled || loading;
		const content = loading ? (
			<>
				<div className="contents !text-transparent invisible">{children}</div>
				<div
					data-testid="loading-indicator"
					className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex"
				>
					<LoadingIndicator color="inherit" />
				</div>
			</>
		) : (
			children
		);
		const styles = loading ? ({ position: 'relative', textOverflow: 'clip' } as const) : undefined;
		// External link:
		if (href && !isDisabled) {
			return (
				<a
					ref={ref as Ref<HTMLAnchorElement>}
					target="_blank"
					rel="noreferrer noopener"
					href={href}
					{...props}
					style={styles}
				>
					{content}
				</a>
			);
		}
		// Internal router link:
		if (to && !isDisabled) {
			return (
				<Link to={to} ref={ref as Ref<HTMLAnchorElement>} {...props} style={styles}>
					{content}
				</Link>
			);
		}
		return (
			<button
				{...props}
				type={props.type || 'button'}
				ref={ref as Ref<HTMLButtonElement>}
				disabled={isDisabled}
				style={styles}
			>
				{content}
			</button>
		);
	},
);
