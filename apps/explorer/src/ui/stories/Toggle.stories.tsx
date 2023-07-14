// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Toggle } from '../Toggle';

export default {
	component: Toggle,
} as Meta;

export const Default: StoryObj<typeof Toggle> = {
	render: (props) => <Toggle {...props} />,
};

export const Checked: StoryObj = {
	...Default,
	args: {
		checked: true,
	},
};
