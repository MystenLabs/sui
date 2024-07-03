// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import type { ReactNode } from 'react';

import { Text } from '../shared/text';

export type SummaryCardProps = {
	header?: string;
	body: ReactNode;
	footer?: ReactNode;
	minimalPadding?: boolean;
	showDivider?: boolean;
	noBorder?: boolean;
	boxShadow?: boolean;
};

export function SummaryCard({
	body,
	header,
	footer,
	minimalPadding,
	showDivider = false,
	noBorder = false,
	boxShadow = false,
}: SummaryCardProps) {
	return (
		<div
			className={clsx(
				{ 'border border-solid border-gray-45': !noBorder, 'shadow-card-soft': boxShadow },
				'bg-white flex flex-col flex-nowrap rounded-2xl w-full',
			)}
		>
			{header ? (
				<div className="flex flex-row flex-nowrap items-center justify-center uppercase bg-gray-40 px-3.75 py-2.5 rounded-t-2xl">
					<Text variant="captionSmall" weight="bold" color="steel-darker" truncate>
						{header}
					</Text>
				</div>
			) : null}
			<div
				className={clsx(
					'flex-1 flex flex-col items-stretch flex-nowrap px-4 overflow-y-auto',
					minimalPadding ? 'py-2' : 'py-4',
					showDivider ? 'divide-x-0 divide-y divide-gray-40 divide-solid' : '',
				)}
			>
				{body}
			</div>
			{footer ? (
				<div className="p-4 pt-3 border-x-0 border-b-0 border-t border-solid border-gray-40">
					{footer}
				</div>
			) : null}
		</div>
	);
}
