// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { TableHeader, type TableHeaderProps } from '../TableHeader';

export default {
	component: TableHeader,
} as Meta;

export const Default: StoryObj<TableHeaderProps> = {
	args: {
		children: 'Table Header',
		after: 'After Content',
	},
};

export const WithSubtext: StoryObj<TableHeaderProps> = {
	args: {
		children: 'Table Header',
		subText: 'Subtext',
		after: 'After Content',
	},
};
