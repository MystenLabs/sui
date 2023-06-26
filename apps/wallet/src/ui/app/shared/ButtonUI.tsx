// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: replace all the existing button usages (the current Button component or button) with this
// TODO: rename this to Button when the existing Button component is removed

import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type Ref } from 'react';

import { ButtonOrLink, type ButtonOrLinkProps } from './utils/ButtonOrLink';

import type { ReactNode } from 'react';

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
					'bg-gray-45 text-steel-darker border-none',
					'visited:text-steel-darker',
					'active:text-steel-dark/70',
					'disabled:bg-gray-40 disabled:text-steel/50',
				],
				outline: [
					'bg-white border-solid border border-steel text-steel-dark',
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
			},
			size: {
				tall: ['h-11 px-5 rounded-xl'],
				narrow: ['h-9 py-2.5 px-5 rounded-lg'],
				tiny: ['h-5 rounded-lg px-2'],
			},
		},
	},
);
const iconStyles = cva('flex', {
	variants: {
		variant: {
			primary: ['text-sui-light group-active:text-steel/70 group-disabled:text-steel/50'],
			secondary: [
				'text-steel',
				'group-hover:text-steel-darker group-focus:text-steel-darker',
				'group-active:text-steel-dark/70',
				'group-disabled:text-steel/50',
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
