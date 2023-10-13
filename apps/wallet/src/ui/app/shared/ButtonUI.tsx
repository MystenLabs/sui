// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: replace all the existing button usages (the current Button component or button) with this
// TODO: rename this to Button when the existing Button component is removed

import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

const styles = cva(
	[
		'transition no-underline outline-none group',
		'flex flex-row flex-nowrap items-center justify-center gap-2',
		'cursor-pointer text-body font-semibold max-w-full min-w-0 w-full',
	],
	{
		variants: {
			variant: {
				primary: [
					'bg-hero-dark text-white border-none',
					'hover:bg-hero focus:bg-hero',
					'visited:text-white',
					'active:text-white/70',
					'disabled:bg-hero-darkest disabled:text-white disabled:opacity-40',
				],
				secondary: [
					'bg-hero-darkest/10 text-steel-dark border-none',
					'hover:bg-hero-darkest/20 hover:text-steel-darker',
					'focus:bg-hero-darkest/10 focus:text-steel-dark/70',
					'active:text-steel-dark/70',
					'visited:text-steel-darkest',
					'disabled:bg-hero-darkest/5 disabled:text-steel/50',
				],
				secondarySui: [
					'bg-transparent text-steel border-none',
					'hover:bg-sui-light focus:bg-sui-light',
					'visited:text-steel-darker',
					'active:text-steel-dark/70',
					'disabled:bg-gray-40 disabled:text-steel/50',
				],
				outline: [
					'bg-white border-solid border border-steel text-steel-dark text-body font-semibold',
					'hover:border-steel-dark focus:border-steel-dark hover:text-steel-darker focus:text-steel-darker',
					'visited:text-steel-dark',
					'active:border-steel active:text-steel-dark',
					'disabled:border-gray-45 disabled:text-gray-60',
				],
				outlineWarning: [
					'bg-white border-solid border border-steel text-issue-dark',
					'hover:border-steel-dark focus:border-steel-dark',
					'visited:text-issue-dark',
					'active:border-steel active:text-issue/70',
					'disabled:border-gray-45 disabled:text-issue-dark/50',
				],
				warning: [
					'bg-issue-light text-issue-dark border-none',
					'visited:text-issue-dark',
					'active:text-issue/70',
					'disabled:opacity-40 disabled:text-issue-dark/50',
				],
				plain: [
					'bg-transparent text-steel-darker border-none',
					'visited:text-steel-darker',
					'active:text-steel-darker/70',
					'disabled:text-steel-dark/50',
				],
				hidden: [
					'bg-gray-45 bg-opacity-25 text-gray-60 hover:text-sui-dark hover:bg-gray-35 hover:bg-opacity-75 border-none h-full w-full backdrop-blur-md',
				],
			},
			size: {
				tall: ['h-10 px-5 rounded-xl'],
				narrow: ['h-9 py-2.5 px-5 rounded-lg'],
				xs: ['h-6 rounded-lg px-2 py-3 !uppercase text-captionSmall'],
				icon: ['h-full w-full rounded-lg p-1'],
			},
		},
	},
);
const iconStyles = cva('flex', {
	variants: {
		border: {
			none: 'border-none',
		},
		variant: {
			primary: ['text-sui-light group-active:text-steel/70 group-disabled:text-steel/50'],
			secondary: [
				'text-steel',
				'group-hover:text-steel-darker group-focus:text-steel-darker',
				'group-active:text-steel-dark/70',
				'group-disabled:text-steel/50',
			],
			secondarySui: [
				'text-steel',
				'group-hover:text-hero group-focus:text-hero',
				'group-active:text-hero/70',
				'group-disabled:text-hero/50',
			],
			outline: [
				'text-steel',
				'group-hover:text-steel-darker group-focus:text-steel-darker',
				'group-active:text-steel-dark',
				'group-disabled:text-gray-45',
			],
			outlineWarning: [
				'text-issue-dark/80',
				'group-hover:text-issue-dark group-focus:text-issue-dark',
				'group-active:text-issue/70',
				'group-disabled:text-issue/50',
			],
			warning: [
				'text-issue-dark/80',
				'group-hover:text-issue-dark group-focus:text-issue-dark',
				'group-active:text-issue/70',
				'group-disabled:text-issue/50',
			],
			plain: [],
			hidden: [],
		},
	},
});

export interface ButtonProps
	extends VariantProps<typeof styles>,
		VariantProps<typeof iconStyles>,
		Omit<ButtonOrLinkProps, 'className'> {
	before?: ReactNode;
	after?: ReactNode;
	text?: ReactNode;
}

export const Button = forwardRef(
	(
		{ variant = 'primary', size = 'narrow', before, after, text, ...otherProps }: ButtonProps,
		ref: Ref<HTMLAnchorElement | HTMLButtonElement>,
	) => {
		return (
			<ButtonOrLink ref={ref} className={styles({ variant, size })} {...otherProps}>
				{before ? <div className={iconStyles({ variant })}>{before}</div> : null}
				{text ? <div className={'truncate'}>{text}</div> : null}
				{after ? <div className={iconStyles({ variant })}>{after}</div> : null}
			</ButtonOrLink>
		);
	},
);
