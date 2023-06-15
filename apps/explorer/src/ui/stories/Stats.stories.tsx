// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Stats, type StatsProps } from '../Stats';

export default {
	component: Stats,
} as Meta;

export const defaultAmount: StoryObj<StatsProps> = {
	render: () => (
		<Stats label="Last Epoch Change" tooltip="Last Epoch Change Tooltip">
			2,334
		</Stats>
	),
};
