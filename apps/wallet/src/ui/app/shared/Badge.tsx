// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

const badgeStyle = cva(
	[
		'text-captionSmallExtra flex uppercase font-medium px-1 py-0.5 rounded w-fit-content h-3.5 w-max justify-center items-center',
	],
	{
		variants: {
			variant: {
				warning: 'bg-issue-light text-issue-dark',
				success: 'bg-sui/30 text-hero-dark',
			},
		},
	},
);

export interface BadgeProps extends VariantProps<typeof badgeStyle> {
	label: string;
}

export function Badge({ label, ...styles }: BadgeProps) {
	return <div className={badgeStyle(styles)}>{label}</div>;
}
