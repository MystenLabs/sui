// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const cardStyles = cva('overflow-hidden', {
	variants: {
		bg: {
			default: 'bg-gray-40',
			highlight: 'bg-success-light',
			lightBlue: 'bg-sui/10',
			white: 'bg-white',
			'white/80': 'bg-white/80',
		},
		height: {
			full: 'h-full',
		},
		rounded: {
			lg: 'rounded-lg',
			xl: 'rounded-xl',
			'2xl': 'rounded-2xl',
		},
		spacing: {
			none: '',
			sm: 'px-5 py-4',
			md: 'p-5',
			lg: 'p-6',
			lgGraph: 'p-6 pb-4',
		},
		border: {
			gray45: 'border border-gray-45',
			steel: 'border border-steel',
		},
		shadow: {
			true: 'shadow',
		},
		growOnHover: {
			true: 'hover:scale-101 ease-in-out duration-200',
		},
	},
	defaultVariants: {
		bg: 'default',
		spacing: 'md',
		rounded: 'xl',
	},
});

export interface CardProps extends VariantProps<typeof cardStyles> {
	children?: ReactNode;
}

export function Card({
	spacing,
	rounded,
	bg,
	border,
	shadow,
	children,
	height,
	growOnHover,
}: CardProps) {
	return (
		<div
			className={cardStyles({
				spacing,
				rounded,
				bg,
				border,
				shadow,
				height,
				growOnHover,
			})}
		>
			{children}
		</div>
	);
}
