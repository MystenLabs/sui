// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

const OwnedObjectsTextStyles = cva(
	['truncate break-words font-sans text-subtitle group-hover:text-hero-dark'],
	{
		variants: {
			color: {
				'steel-dark': 'text-steel-dark',
				'steel-darker': 'text-steel-darker',
			},
			font: {
				semibold: 'font-semibold',
				medium: 'font-medium',
				normal: 'font-normal',
			},
		},
	},
);

type OwnedObjectsTextStylesProps = VariantProps<typeof OwnedObjectsTextStyles>;

interface OwnedObjectsTextProps extends OwnedObjectsTextStylesProps {
	children: ReactNode;
}

export function OwnedObjectsText({ color, font, children }: OwnedObjectsTextProps) {
	return <div className={OwnedObjectsTextStyles({ color, font })}>{children}</div>;
}
