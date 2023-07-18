// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading, type HeadingProps } from '@mysten/ui';
import { type ReactNode } from 'react';

export interface TableHeaderProps extends Pick<HeadingProps, 'as'> {
	children: ReactNode;
	subText?: ReactNode;
	after?: ReactNode;
}

export function TableHeader({ as = 'h3', children, subText, after }: TableHeaderProps) {
	return (
		<div className="flex items-center border-b border-gray-45 pb-5">
			<div className="flex flex-1">
				<Heading as={as} variant="heading4/semibold" color="gray-90">
					{children}
				</Heading>
				{subText && (
					<Heading variant="heading4/medium" color="steel-dark">
						&nbsp;{subText}
					</Heading>
				)}
			</div>
			{after && <div className="flex items-center">{after}</div>}
		</div>
	);
}
