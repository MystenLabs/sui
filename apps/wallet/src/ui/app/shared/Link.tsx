// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const styles = cva(
	[
		'transition flex flex-nowrap items-center justify-center outline-none gap-1 w-full',
		'bg-transparent p-0 border-none',
		'active:opacity-70',
		'disabled:opacity-40',
		'cursor-pointer group',
	],
	{
		variants: {
			underline: {
				none: 'no-underline',
				hover: 'no-underline hover:underline',
			},
			color: {
				steelDark: [
					'text-steel-dark hover:text-steel-darker focus:text-steel-darker disabled:text-steel-dark',
				],
				steelDarker: [
					'text-steel-darker hover:text-steel-darker focus:text-steel-darker disabled:text-steel-darker',
				],
				heroDark: [
					'text-hero-dark hover:text-hero-darkest focus:text-hero-darkest disabled:text-hero-dark',
				],
				suiDark: ['text-sui-dark'],
				hero: ['text-hero hover:text-hero-dark focus:text-hero-dark disabled:text-hero-dark'],
			},
			weight: {
				semibold: 'font-semibold',
				medium: 'font-medium',
			},
			size: {
				bodySmall: 'text-bodySmall',
				body: 'text-body',
				base: 'text-base leading-none',
				captionSmall: 'text-captionSmall',
			},
			mono: {
				true: 'font-mono',
				false: '',
			},
		},
	},
);

const iconStyles = cva(['transition flex'], {
	variants: {
		beforeColor: {
			steelDarker: ['text-steel-darker'],
		},
		color: {
			steelDark: [
				'text-steel group-hover:text-steel-darker group-focus:text-steel-darker group-disabled:text-steel-dark',
			],
			steelDarker: [
				'text-steel-darker group-hover:text-steel-darker group-focus:text-steel-darker group-disabled:text-steel-darker',
			],
			heroDark: [
				'text-hero group-hover:text-hero-darkest group-focus:text-hero-darkest group-disabled:text-hero-dark',
			],
			suiDark: [
				'text-steel group-hover:text-sui-dark group-focus:text-sui-dark group-disabled:text-steel',
			],
			hero: [
				'text-hero group-hover:text-hero-dark group-focus:text-hero-dark group-disabled:text-hero-dark',
			],
		},
	},
});

export interface LinkProps
	extends VariantProps<typeof styles>,
		VariantProps<typeof iconStyles>,
		Omit<ButtonOrLinkProps, 'className' | 'color'> {
	before?: ReactNode;
	after?: ReactNode;
	text?: ReactNode;
}

export const Link = forwardRef(
	(
		{
			before,
			beforeColor,
			after,
			text,
			color,
			weight,
			size = 'bodySmall',
			underline = 'none',
			mono,
			...otherProps
		}: LinkProps,
		ref: Ref<HTMLAnchorElement | HTMLButtonElement>,
	) => (
		<ButtonOrLink
			className={styles({ color, weight, size, underline, mono })}
			{...otherProps}
			ref={ref}
		>
			{before ? (
				<div className={beforeColor ? iconStyles({ beforeColor }) : iconStyles({ color })}>
					{before}
				</div>
			) : null}
			{text ? <div className={'truncate leading-none'}>{text}</div> : null}
			{after ? <div className={iconStyles({ color })}>{after}</div> : null}
		</ButtonOrLink>
	),
);
