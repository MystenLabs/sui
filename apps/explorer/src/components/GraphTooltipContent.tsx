// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTooltipPosition } from '@visx/tooltip';
import clsx from 'clsx';
import { type ReactNode } from 'react';

export type GraphTooltipContentProps = {
	children: ReactNode;
};

export function GraphTooltipContent({ children }: GraphTooltipContentProps) {
	const { isFlippedHorizontally } = useTooltipPosition();
	return (
		<div
			className={clsx(
				'w-fit -translate-y-[calc(100%-10px)] rounded-md border border-solid border-steel bg-white/90 p-2 shadow-mistyEdge',
				isFlippedHorizontally
					? '-translate-x-[1px] rounded-bl-none'
					: 'translate-x-[1px] rounded-br-none',
			)}
		>
			{children}
		</div>
	);
}
