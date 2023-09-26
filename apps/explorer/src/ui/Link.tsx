// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const linkStyles = cva([], {
	variants: {
		variant: {
			text: 'text-body font-semibold text-steel-dark hover:text-steel-darker active:text-steel disabled:text-gray-60',
			mono: 'font-mono text-body font-medium text-hero-dark hover:text-hero-darkest break-all',
			textHeroDark: 'text-pBody font-medium text-hero-dark hover:text-hero-darkest',
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

const linkContentStyles = cva(['flex-nowrap items-center'], {
	variants: {
		gap: {
			'gap-1': 'gap-1',
			'gap-2': 'gap-2',
		},
		display: {
			'inline-flex': 'inline-flex',
			block: 'block',
			flex: 'flex',
		},
	},
	defaultVariants: {
		gap: 'gap-2',
		display: 'inline-flex',
	},
});

type LinkContentStylesProps = VariantProps<typeof linkContentStyles>;

export interface LinkProps
	extends ButtonOrLinkProps,
		VariantProps<typeof linkStyles>,
		LinkContentStylesProps {
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
	display,
	gap,
	...props
}: LinkProps) {
	return (
		<ButtonOrLink className={linkStyles({ variant, size, uppercase })} {...props}>
			<div className={linkContentStyles({ gap, display })}>
				{before}
				{children}
				{after}
			</div>
		</ButtonOrLink>
	);
}
