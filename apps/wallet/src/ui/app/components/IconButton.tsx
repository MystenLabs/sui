// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { ButtonOrLink, type ButtonOrLinkProps } from '../shared/utils/ButtonOrLink';

interface IconButtonProps extends ButtonOrLinkProps, VariantProps<typeof buttonStyles> {
	icon: JSX.Element;
}

const buttonStyles = cva(
	[
		'flex items-center rounded-sm bg-transparent border-0 p-0 text-hero-darkest/40 hover:text-hero-darkest/50 transition cursor-pointer',
	],
	{
		variants: {
			variant: {
				transparent: '',
				subtle: 'hover:bg-hero-darkest/10',
			},
		},
		defaultVariants: {
			variant: 'subtle',
		},
	},
);

export function IconButton({ onClick, icon, variant, ...buttonOrLinkProps }: IconButtonProps) {
	return (
		<ButtonOrLink
			onClick={onClick}
			className={buttonStyles({ variant })}
			children={icon}
			{...buttonOrLinkProps}
		/>
	);
}
