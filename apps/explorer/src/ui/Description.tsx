// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text, type TextProps } from '@mysten/ui';
import clsx from 'clsx';
import { type ReactNode } from 'react';

interface DescriptionProps {
	title: ReactNode;
	children: ReactNode;
	titleVariant?: TextProps['variant'];
	titleColor?: TextProps['color'];
	alignItems?: 'start' | 'center';
}

export function Description({
	title,
	children,
	titleVariant = 'pBodySmall/medium',
	titleColor = 'steel-dark',
	alignItems = 'start',
}: DescriptionProps) {
	return (
		<div
			className={clsx(
				'flex justify-between gap-10',
				alignItems === 'center' && 'items-center',
				alignItems === 'start' && 'items-start',
			)}
		>
			<Text variant={titleVariant} color={titleColor}>
				{title}
			</Text>

			{children}
		</div>
	);
}
