// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16, Search16 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { Link, type LinkProps } from '../Link';

export default {
	component: Link,
} as Meta;

export const Text: StoryObj<LinkProps> = {
	args: {
		variant: 'text',
		children: 'View more',
	},
};

export const TextDisabled: StoryObj<LinkProps> = {
	args: {
		variant: 'text',
		children: 'View more',
		disabled: true,
	},
};

export const Mono: StoryObj<LinkProps> = {
	args: {
		variant: 'mono',
		children: '0x0000000000000000000000000000000000000002',
	},
};

export const LinkWithPrefixIcon: StoryObj<LinkProps> = {
	args: {
		variant: 'text',
		children: 'View more',
		before: <CheckFill16 />,
	},
};

export const LinkWithPostfixIcon: StoryObj<LinkProps> = {
	args: {
		variant: 'mono',
		children: '0x0000000000000000000000000000000000000002',
		after: <Search16 />,
	},
};

export const LinkWithIcons: StoryObj<LinkProps> = {
	args: {
		variant: 'text',
		children: 'View more',
		before: <CheckFill16 />,
		after: <Search16 />,
	},
};
