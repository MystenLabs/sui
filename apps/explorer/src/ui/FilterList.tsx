// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: This component really shouldn't use the `Tabs` component, it should just use radix,
// and should define it's own styles since the concerns here are pretty different.

import { type ComponentProps } from 'react';

import { TabsList, Tabs, TabsTrigger } from './Tabs';

export interface FilterListProps<T extends string = string> {
	options: readonly T[];
	value: T;
	disabled?: boolean;
	size?: ComponentProps<typeof Tabs>['size'];
	lessSpacing?: ComponentProps<typeof TabsList>['lessSpacing'];
	onChange(value: T): void;
}

export function FilterList<T extends string>({
	options,
	value,
	disabled = false,
	size,
	lessSpacing,
	onChange,
}: FilterListProps<T>) {
	return (
		<Tabs
			size={size}
			value={value}
			onValueChange={(value) => {
				onChange(value as T);
			}}
		>
			<TabsList disableBottomBorder lessSpacing={lessSpacing}>
				{options.map((option) => (
					<TabsTrigger disabled={disabled} key={option} value={option}>
						{option}
					</TabsTrigger>
				))}
			</TabsList>
		</Tabs>
	);
}
