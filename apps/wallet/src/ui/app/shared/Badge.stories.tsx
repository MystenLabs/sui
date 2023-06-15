// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Badge } from './Badge';

export default {
	component: Badge,
} as Meta<typeof Badge>;

export const Success: StoryObj<typeof Badge> = {
	args: {
		label: 'New',
		variant: 'success',
	},
};

export const Warning: StoryObj<typeof Badge> = {
	args: {
		label: 'At Risk',
		variant: 'warning',
	},
};
