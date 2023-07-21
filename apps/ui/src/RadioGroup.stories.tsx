// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';

import { RadioGroup, RadioGroupItem } from './RadioGroup';

const meta = {
	component: RadioGroup,
} satisfies Meta<typeof RadioGroup>;

export default meta;

const groups = [
	{
		value: '1',
		label: 'label 1',
		description: 'description 1',
	},
	{
		value: '2',
		label: 'label 2',
		description: 'description 2',
	},
	{
		value: '3',
		label: 'label 3',
		description: 'description 3',
	},
];

type Story = StoryObj<typeof meta>;

export const Default: Story = {
	args: {
		'aria-label': 'Default radio group',
		children: groups.map((group) => (
			<RadioGroupItem key={group.label} value={group.value} label={group.label} />
		)),
	},
};
