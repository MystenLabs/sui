// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text, type TextProps } from '@mysten/ui';
import { type ReactNode } from 'react';

interface DescriptionProps {
	title: string;
	children: ReactNode;
	titleVariant?: TextProps['variant'];
	titleColor?: TextProps['color'];
}

export function Description({
	title,
	children,
	titleVariant = 'pBodySmall/medium',
	titleColor = 'steel-dark',
}: DescriptionProps) {
	return (
		<div className="flex items-start justify-between gap-10">
			<Text variant={titleVariant} color={titleColor}>
				{title}
			</Text>

			{children}
		</div>
	);
}
