// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Input, type InputProps } from '../Input';

import type { Meta, StoryObj } from '@storybook/react';

export default {
	component: Input,
} as Meta;

export const InputDefault: StoryObj<InputProps> = {
	render: (props) => <Input {...props} />,
	args: {
		value: 'Test value',
		label: 'Test label',
	},
};

export const InputPlaceholder: StoryObj<InputProps> = {
	...InputDefault,
	args: {
		value: undefined,
		placeholder: 'Test placeholder',
		label: 'Input with placeholder',
	},
};

export const InputDisabled: StoryObj<InputProps> = {
	...InputDefault,
	args: {
		disabled: true,
		label: 'Disabled input',
	},
};
