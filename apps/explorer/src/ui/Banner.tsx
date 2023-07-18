// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X12 } from '@mysten/icons';
import { Text, IconButton } from '@mysten/ui';
import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { ReactComponent as InfoIcon } from './icons/info.svg';

const bannerStyles = cva(
	'inline-flex text-pBodySmall font-medium rounded-2xl overflow-hidden gap-2 items-center flex-nowrap relative',
	{
		variants: {
			variant: {
				positive: 'bg-success-light text-success-dark',
				warning: 'bg-warning-light text-warning-dark',
				error: 'bg-issue-light text-issue-dark',
				message: 'bg-sui-light text-hero',
				neutralGrey: 'bg-steel text-white',
				neutralWhite: 'bg-white text-steel-darker',
			},
			align: {
				left: 'justify-start',
				center: 'justify-center',
			},
			fullWidth: {
				true: 'w-full',
			},
			spacing: {
				md: 'px-3 py-2',
				lg: 'p-5',
			},
			shadow: {
				true: 'shadow-md',
			},
			border: {
				true: '',
			},
		},
		defaultVariants: {
			variant: 'message',
			spacing: 'md',
		},
		compoundVariants: [
			{
				variant: 'positive',
				border: true,
				class: 'border border-success/30',
			},
			{
				variant: 'warning',
				border: true,
				class: 'border border-warning-dark/30',
			},
			{
				variant: 'error',
				border: true,
				class: 'border border-issue-dark/30',
			},
			{
				variant: 'message',
				border: true,
				class: 'border border-sui/30',
			},
			{
				variant: 'neutralGrey',
				border: true,
				class: 'border border-steel',
			},
			{
				variant: 'neutralWhite',
				border: true,
				class: 'border border-gray-45',
			},
		],
	},
);

export interface BannerProps extends VariantProps<typeof bannerStyles> {
	icon?: ReactNode | null;
	title?: ReactNode | string;
	children: ReactNode;
	onDismiss?: () => void;
}

export function Banner({
	icon = <InfoIcon />,
	title,
	children,
	variant,
	align,
	fullWidth,
	spacing,
	border,
	shadow,
	onDismiss,
}: BannerProps) {
	return (
		<div
			className={bannerStyles({
				variant,
				align,
				fullWidth,
				shadow,
				border,
				spacing,
				class: onDismiss && 'pr-9',
			})}
		>
			{icon && <div className="flex items-center justify-center">{icon}</div>}
			<div className="flex flex-col gap-1">
				{title && <Text variant="bodySmall/semibold">{title}</Text>}
				<div className="overflow-hidden break-words break-all">{children}</div>
			</div>
			{onDismiss ? (
				<div className="absolute right-0 top-0">
					<IconButton onClick={onDismiss} aria-label="Close">
						<X12 />
					</IconButton>
				</div>
			) : null}
		</div>
	);
}
