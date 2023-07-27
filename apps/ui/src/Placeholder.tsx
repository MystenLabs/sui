// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, VariantProps } from 'class-variance-authority';

const placeholderStyle = cva(
	'h-[1em] w-full animate-shimmer rounded-[3px] bg-placeholderShimmer bg-[length:1000px_100%]',
	{
		variants: {
			rounded: {
				'3px': 'rounded-[3px]',
				lg: 'rounded-lg',
				xl: 'rounded-xl',
			},
		},
		defaultVariants: {
			rounded: '3px',
		},
	},
);

type PlaceholderStyleProps = VariantProps<typeof placeholderStyle>;

export interface PlaceholderProps extends PlaceholderStyleProps {
	width?: string;
	height?: string;
}

export function Placeholder({ rounded, width = '100%', height = '1em' }: PlaceholderProps) {
	return <div className={placeholderStyle({ rounded })} style={{ width, height }} />;
}
