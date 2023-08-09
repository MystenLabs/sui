// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Tabs, TabsContent, TabsList, TabsTrigger } from '../Tabs';

export default {
	component: Tabs,
} as Meta;

export const Default: StoryObj<typeof Tabs> = {
	render: (props) => (
		<Tabs defaultValue="1" {...props}>
			<TabsList>
				<TabsTrigger value="1">Tab 1</TabsTrigger>
				<TabsTrigger value="2">Tab 2</TabsTrigger>
				<TabsTrigger value="3">Tab 3</TabsTrigger>
			</TabsList>
			<TabsContent value="1">Tab Panel 1</TabsContent>
			<TabsContent value="2">Tab Panel 2</TabsContent>
			<TabsContent value="3">Tab Panel 3</TabsContent>
		</Tabs>
	),
};

export const Large: StoryObj = {
	...Default,
	args: {
		size: 'lg',
	},
};
