// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';
import { type ReactNode } from 'react';

import { ExpandableList } from '../ExpandableList';
import { type InputProps } from '../Input';

import type { Meta, StoryObj } from '@storybook/react';

export default {
	component: ExpandableList,
} as Meta;

function ListItem({ children }: { children: ReactNode }) {
	return (
		<li>
			<Text color="steel-darker" variant="bodySmall/normal">
				{children}
			</Text>
		</li>
	);
}

export const Default: StoryObj<InputProps> = {
	render: () => (
		<ul>
			<ExpandableList
				defaultItemsToShow={3}
				items={Array.from({ length: 10 }).map((_, index) => (
					<ListItem key={index}>Item {index}</ListItem>
				))}
			/>
		</ul>
	),
};
