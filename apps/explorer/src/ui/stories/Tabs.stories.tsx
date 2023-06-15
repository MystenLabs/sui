// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { TabGroup, TabList, Tab, TabPanels, TabPanel, type TabGroupProps } from '../Tabs';

export default {
	component: TabGroup,
} as Meta;

export const Default: StoryObj<TabGroupProps> = {
	render: (props) => (
		<TabGroup {...props}>
			<TabList>
				<Tab>Tab 1</Tab>
				<Tab>Tab 2</Tab>
				<Tab>Tab 3</Tab>
			</TabList>
			<TabPanels>
				<TabPanel>Tab Panel 1</TabPanel>
				<TabPanel>Tab Panel 2</TabPanel>
				<TabPanel>Tab Panel 3</TabPanel>
			</TabPanels>
		</TabGroup>
	),
};

export const Large: StoryObj<TabGroupProps> = {
	...Default,
	args: {
		size: 'lg',
	},
};
