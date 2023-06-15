// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { NavItem, type NavItemProps } from '../../header/NavItem';
import { ReactComponent as CheckIcon } from '../../icons/check_24x24.svg';

export default {
	component: NavItem,
	decorators: [
		(Story) => (
			<div className="bg-headerNav p-6">
				<Story />
			</div>
		),
	],
} as Meta;

export const Default: StoryObj<NavItemProps> = {
	args: {
		children: 'Nav Item',
	},
};

export const BeforeIcon: StoryObj<NavItemProps> = {
	args: {
		beforeIcon: <CheckIcon />,
		children: 'Nav Item',
	},
};
